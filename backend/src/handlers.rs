//! Axum routes: state, chat, tick, schema synthesis, reset, SSE event stream.

use std::{convert::Infallible, net::SocketAddr};

use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
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
    models::{ChatRequest, Graph, RoomEvent, RoomState, SchemaDoc, SettingsPatch, SettingsView, USER_ID},
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
        .route("/api/settings", get(get_settings).post(post_settings))
        .with_state(state)
}

async fn health(State(app): State<AppState>) -> Json<serde_json::Value> {
    let s = app.llm_settings().await;
    Json(json!({
        "ok": true,
        "version": crate::update::VERSION,
        "mock_mode": s.mock_mode(),
        "model": s.model,
        "director_model": s.director_model,
    }))
}

/// Settings are editable from the machine the server runs on. Behind a reverse proxy the
/// peer is loopback too, so a forwarded-for header demotes the request to "remote".
fn settings_editable(app: &AppState, peer: SocketAddr, headers: &HeaderMap) -> bool {
    if app.cfg.allow_remote_settings {
        return true;
    }
    let forwarded = headers.contains_key("x-forwarded-for") || headers.contains_key("x-real-ip");
    peer.ip().is_loopback() && !forwarded
}

pub async fn build_state(app: &AppState, editable: bool) -> anyhow::Result<RoomState> {
    let nodes = db::list_nodes(&app.db).await?;
    let edges = db::list_edges(&app.db).await?;
    let graph_mermaid = mermaid::build_graph(&nodes, &edges);
    let s = app.llm_settings().await;
    Ok(RoomState {
        agents: db::list_agents(&app.db).await?,
        messages: db::recent_messages(&app.db, 80).await?,
        graph: Graph { nodes, edges },
        graph_mermaid,
        schema: db::last_schema(&app.db).await?,
        last_director: db::last_director_log(&app.db).await?,
        mock_mode: s.mock_mode(),
        model: s.model,
        director_model: s.director_model,
        version: crate::update::VERSION.into(),
        update: app.update.read().await.clone(),
        settings_editable: editable,
    })
}

async fn get_state(
    State(app): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> AppResult<Json<RoomState>> {
    let editable = settings_editable(&app, peer, &headers);
    Ok(Json(build_state(&app, editable).await?))
}

async fn settings_view(app: &AppState, editable: bool) -> SettingsView {
    let s = app.llm_settings().await;
    SettingsView {
        has_key: !s.mock_mode(),
        key_hint: s.key_hint(),
        model: s.model,
        director_model: s.director_model,
        editable,
        config_path: crate::config::config_path().display().to_string(),
        version: crate::update::VERSION.into(),
        update: app.update.read().await.clone(),
        ui_embedded: crate::web::is_embedded(),
    }
}

async fn get_settings(
    State(app): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Json<SettingsView> {
    let editable = settings_editable(&app, peer, &headers);
    Json(settings_view(&app, editable).await)
}

async fn post_settings(
    State(app): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(patch): Json<SettingsPatch>,
) -> Result<Json<SettingsView>, (StatusCode, Json<serde_json::Value>)> {
    if !settings_editable(&app, peer, &headers) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error": "Настройки можно менять только с машины, где запущен сервер (или ALLOW_REMOTE_SETTINGS=true). Используй `triad setup` / `triad key`."})),
        ));
    }
    let mut s = app.llm_settings().await;
    if patch.clear_key {
        s.api_key = None;
    } else if let Some(k) = patch.api_key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty()) {
        if k.chars().count() < 16 || k.chars().any(char::is_whitespace) {
            return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "ключ выглядит неправильно"}))));
        }
        s.api_key = Some(k);
    }
    if let Some(m) = patch.model.map(|m| m.trim().to_string()).filter(|m| !m.is_empty()) {
        s.model = m;
    }
    if let Some(m) = patch.director_model.map(|m| m.trim().to_string()).filter(|m| !m.is_empty()) {
        s.director_model = m;
    }
    if let Err(e) = s.save() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("не удалось сохранить конфиг: {e:#}")}))));
    }
    *app.settings.write().await = s;
    tracing::info!("LLM settings updated via UI");
    Ok(Json(settings_view(&app, true).await))
}

async fn chat(
    State(app): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<ChatRequest>,
) -> AppResult<Json<RoomState>> {
    let editable = settings_editable(&app, peer, &headers);
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
    Ok(Json(build_state(&app, editable).await?))
}

async fn tick(
    State(app): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> AppResult<Json<RoomState>> {
    let editable = settings_editable(&app, peer, &headers);
    let _turn = app.turn_lock.lock().await;
    let alive = db::list_agents(&app.db).await?.into_iter().filter(|a| a.is_alive()).count();
    if alive == 0 {
        return Err(AppError::bad_request("в комнате никого не осталось — нажми «Новая комната»"));
    }
    run_round(&app, Trigger::Tick).await?;
    Ok(Json(build_state(&app, editable).await?))
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

async fn synthesize_schema(
    State(app): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> AppResult<Json<RoomState>> {
    let editable = settings_editable(&app, peer, &headers);
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
    let model = app.llm_settings().await.model;
    let parsed: Option<SchemaDoc> = match app.llm.chat(&model, &msgs, &opts).await {
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
    Ok(Json(build_state(&app, editable).await?))
}

async fn reset(
    State(app): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> AppResult<Json<RoomState>> {
    let editable = settings_editable(&app, peer, &headers);
    let _turn = app.turn_lock.lock().await;
    db::reset_room(&app.db).await?;
    let state = build_state(&app, editable).await?;
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
