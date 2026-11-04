//! Axum routes: state, chat, tick, schema synthesis, reset, SSE event stream.

use std::convert::Infallible;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::{Stream, StreamExt};
use serde_json::json;
use tokio_stream::wrappers::BroadcastStream;

use crate::{
    agent_runner, db,
    director::{self, Trigger},
    error::{AppError, AppResult},
    mermaid,
    models::{ChatRequest, Graph, RoomEvent, RoomState, SchemaDoc, USER_ID},
    openrouter::{extract_json, ChatMessage, ChatOpts},
    prompts,
    state::AppState,
};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/state", get(get_state))
        .route("/api/chat", post(chat))
        .route("/api/tick", post(tick))
        .route("/api/schema", post(synthesize_schema))
        .route("/api/synthesize-schema", post(synthesize_schema))
        .route("/api/reset", post(reset))
        .route("/api/events", get(events))
        .with_state(state)
}

async fn health(State(app): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "ok": true,
        "mock_mode": app.llm.is_mock(),
        "model": app.cfg.openrouter_model,
        "director_model": app.cfg.director_model,
    }))
}

pub async fn build_state(app: &AppState) -> anyhow::Result<RoomState> {
    let nodes = db::list_nodes(&app.db).await?;
    let edges = db::list_edges(&app.db).await?;
    let graph_mermaid = mermaid::build_graph(&nodes, &edges);
    Ok(RoomState {
        agents: db::list_agents(&app.db).await?,
        messages: db::recent_messages(&app.db, 80).await?,
        graph: Graph { nodes, edges },
        graph_mermaid,
        schema: db::last_schema(&app.db).await?,
        last_director: db::last_director_log(&app.db).await?,
        mock_mode: app.llm.is_mock(),
        model: app.cfg.openrouter_model.clone(),
        director_model: app.cfg.director_model.clone(),
    })
}

async fn get_state(State(app): State<AppState>) -> AppResult<Json<RoomState>> {
    Ok(Json(build_state(&app).await?))
}

async fn chat(State(app): State<AppState>, Json(req): Json<ChatRequest>) -> AppResult<Json<RoomState>> {
    let text = req.message.trim();
    if text.is_empty() {
        return Err(AppError::bad_request("пустое сообщение"));
    }
    if text.chars().count() > 4000 {
        return Err(AppError::bad_request("сообщение длиннее 4000 символов"));
    }
    let _turn = app.turn_lock.lock().await;
    let msg = db::insert_message(&app.db, USER_ID, None, text).await?;
    app.emit(RoomEvent::Message { message: msg });
    run_round(&app, Trigger::UserMessage).await?;
    Ok(Json(build_state(&app).await?))
}

async fn tick(State(app): State<AppState>) -> AppResult<Json<RoomState>> {
    let _turn = app.turn_lock.lock().await;
    let alive = db::list_agents(&app.db).await?.into_iter().filter(|a| a.is_alive()).count();
    if alive == 0 {
        return Err(AppError::bad_request("в комнате никого не осталось — нажми «Новая комната»"));
    }
    run_round(&app, Trigger::Tick).await?;
    Ok(Json(build_state(&app).await?))
}

/// Director → (1–2) agents, sequentially, so the second can answer the first.
async fn run_round(app: &AppState, trigger: Trigger) -> anyhow::Result<()> {
    app.emit(RoomEvent::Status {
        stage: "director".into(),
        detail: "Режиссёр читает комнату и раздаёт скрытые цели…".into(),
    });
    let entry = director::run_director(app, trigger).await?;
    let mood = entry.output.room_mood.clone();
    let agents = db::list_agents(&app.db).await?;
    for id in &entry.output.next_speakers {
        let name = prompts::sender_name(id, &agents);
        app.emit(RoomEvent::Status {
            stage: "agent".into(),
            detail: format!("{name} формулирует…"),
        });
        if let Err(e) = agent_runner::run_agent(app, id, mood.as_deref()).await {
            tracing::warn!(agent = %id, error = %e, "agent turn skipped");
        }
    }
    app.emit(RoomEvent::Done);
    Ok(())
}

async fn synthesize_schema(State(app): State<AppState>) -> AppResult<Json<RoomState>> {
    let _turn = app.turn_lock.lock().await;
    app.emit(RoomEvent::Status {
        stage: "schema".into(),
        detail: "Триада собирает блок-схему спора…".into(),
    });
    let agents = db::list_agents(&app.db).await?;
    let messages = db::recent_messages(&app.db, app.cfg.context_messages * 2).await?;
    let nodes = db::list_nodes(&app.db).await?;
    let edges = db::list_edges(&app.db).await?;
    let msgs = [
        ChatMessage::system(prompts::schema_system()),
        ChatMessage::user(prompts::schema_user(&agents, &messages, &nodes, &edges)),
    ];
    let opts = ChatOpts {
        temperature: 0.7,
        max_tokens: 2200,
        json_mode: true,
        prefill: None,
    };
    let parsed: Option<SchemaDoc> = match app.llm.chat(&app.cfg.openrouter_model, &msgs, &opts).await {
        Ok(raw) => extract_json(&raw)
            .ok()
            .and_then(|v| serde_json::from_value::<SchemaDoc>(v).ok())
            .and_then(|mut d| {
                let m = mermaid::sanitize_llm_mermaid(&d.mermaid)?;
                d.mermaid = m;
                if d.title.trim().is_empty() {
                    d.title = "Схема спора".into();
                }
                d.annotations.retain(|a| !a.text.trim().is_empty());
                for a in d.annotations.iter_mut() {
                    a.author = mermaid::normalize_author(&a.author).to_string();
                }
                Some(d)
            }),
        Err(e) => {
            tracing::warn!(error = %e, "schema LLM call failed");
            None
        }
    };
    let doc = parsed.unwrap_or_else(|| SchemaDoc {
        title: "Автосборка из графа памяти (модель не ответила)".into(),
        mermaid: mermaid::build_graph(&nodes, &edges),
        annotations: vec![],
        created_at: String::new(),
    });
    let saved = db::save_schema(&app.db, &doc).await?;
    app.emit(RoomEvent::Schema { schema: saved });
    app.emit(RoomEvent::Done);
    Ok(Json(build_state(&app).await?))
}

async fn reset(State(app): State<AppState>) -> AppResult<Json<RoomState>> {
    let _turn = app.turn_lock.lock().await;
    db::reset_room(&app.db).await?;
    let state = build_state(&app).await?;
    app.emit(RoomEvent::Agents { agents: state.agents.clone() });
    app.emit(RoomEvent::Graph {
        graph: state.graph.clone(),
        mermaid: state.graph_mermaid.clone(),
    });
    app.emit(RoomEvent::Done);
    Ok(Json(state))
}

/// Server-Sent Events: live progress while a round runs (works through any WebView/proxy).
async fn events(State(app): State<AppState>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = app.events.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|item| async move {
        let ev = item.ok()?;
        let event = Event::default()
            .json_data(&ev)
            .unwrap_or_else(|_| Event::default().data("{}"));
        Some(Ok::<Event, Infallible>(event))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
