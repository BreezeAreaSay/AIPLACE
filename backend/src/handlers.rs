//! Axum routes: state, chat, tick, schema synthesis, reset, SSE event stream.

use std::{convert::Infallible, net::SocketAddr};

use axum::{
    async_trait,
    extract::{ConnectInfo, FromRequestParts, Path, State},
    http::{header, request::Parts, HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{delete, get, post},
    Json, Router,
};
use futures::{Stream, StreamExt};
use serde_json::json;
use tokio_stream::wrappers::BroadcastStream;

use crate::{
    agent_runner, contracts, db,
    director::{self, Trigger},
    error::{AppError, AppResult},
    memory, mermaid,
    models::{
        AskNodeRequest, ChatRequest, CreateEdgeRequest, CreateNodeRequest, Graph, NoteRequest, PatchNodeRequest,
        ResolveContractRequest, RoomEvent, RoomState, SchemaDoc, SettingsPatch, SettingsView, USER_ID,
    },
    openrouter::{extract_json, ChatMessage, ChatOpts},
    prompts,
    rooms::LOCAL_ROOM,
    state::AppState,
    telegram::{self, TgUser},
};

/// Per-request room scope: resolves the Telegram user (if any), picks their room and decides
/// whether this client may edit server settings.
pub struct Room {
    pub app: AppState,
    pub user: Option<TgUser>,
    pub editable: bool,
}

fn is_forwarded(headers: &HeaderMap) -> bool {
    headers.contains_key("x-forwarded-for") || headers.contains_key("x-real-ip")
}

#[async_trait]
impl FromRequestParts<AppState> for Room {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let peer = parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|c| c.0);
        let loopback = peer.map(|p| p.ip().is_loopback()).unwrap_or(false) && !is_forwarded(&parts.headers);
        let init = telegram::extract_init_data(&parts.headers, parts.uri.query());
        let mut user: Option<TgUser> = None;
        match (&state.cfg.telegram_bot_token, init) {
            (Some(token), Some(raw)) => {
                let u = telegram::validate_init_data(&raw, token, 86_400)
                    .map_err(|e| AppError::unauthorized(format!("Telegram initData отклонён: {e}"), "telegram_invalid"))?;
                user = Some(u);
            }
            (Some(_), None) if !loopback && !state.cfg.allow_anon_rooms => {
                return Err(AppError::unauthorized(
                    "Эта комната открывается только из Telegram Mini App.",
                    "telegram_required",
                ));
            }
            _ => {}
        }
        let room_id = user.as_ref().map(telegram::room_id_for).unwrap_or_else(|| LOCAL_ROOM.to_string());
        let app = state.for_room(&room_id).await?;
        let admin = user.as_ref().map(|u| state.cfg.telegram_admin_ids.contains(&u.id)).unwrap_or(false);
        let editable = state.cfg.allow_remote_settings || admin || loopback;
        Ok(Room { app, user, editable })
    }
}

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
        .route("/api/contracts/:id/resolve", post(resolve_contract))
        .route("/api/nodes", post(create_node))
        .route("/api/nodes/:id", axum::routing::patch(patch_node).delete(delete_node))
        .route("/api/nodes/:id/ask", post(ask_node))
        .route("/api/nodes/:id/notes", post(add_note))
        .route("/api/edges", post(create_edge))
        .route("/api/edges/:id", delete(delete_edge))
        .route("/api/export.md", get(export_markdown))
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
        "telegram": {
            "enabled": app.cfg.telegram_bot_token.is_some(),
            "bot_username": app.bot_username.read().await.clone(),
            "anon_rooms": app.cfg.allow_anon_rooms,
        },
    }))
}

pub async fn build_state(app: &AppState, editable: bool, user: Option<&TgUser>) -> anyhow::Result<RoomState> {
    let nodes = db::list_nodes(&app.db).await?;
    let edges = db::list_edges(&app.db).await?;
    let graph_mermaid = mermaid::build_graph(&nodes, &edges);
    let s = app.llm_settings().await;
    Ok(RoomState {
        agents: db::list_agents(&app.db).await?,
        messages: db::recent_messages(&app.db, 80).await?,
        graph: Graph { nodes, edges },
        node_notes: db::list_notes(&app.db, 400).await?,
        graph_mermaid,
        schema: db::last_schema(&app.db).await?,
        last_director: db::last_director_log(&app.db).await?,
        mock_mode: s.mock_mode(),
        model: s.model,
        director_model: s.director_model,
        version: crate::update::VERSION.into(),
        update: app.update.read().await.clone(),
        settings_editable: editable,
        contracts: db::list_contracts(&app.db).await?,
        relations: db::list_relations(&app.db).await?,
        memories: db::list_memories(&app.db).await?,
        events: db::recent_events(&app.db, 20).await?,
        now: contracts::now_rfc3339(),
        room_id: app.room_id.clone(),
        tg_user: user.cloned(),
    })
}

async fn get_state(Room { app, editable, user }: Room) -> AppResult<Json<RoomState>> {
    if app.turn_lock.try_lock().is_ok() {
        contracts::expire_overdue(&app).await?;
    }
    Ok(Json(build_state(&app, editable, user.as_ref()).await?))
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

async fn get_settings(Room { app, editable, .. }: Room) -> Json<SettingsView> {
    Json(settings_view(&app, editable).await)
}

async fn post_settings(
    Room { app, editable, .. }: Room,
    Json(patch): Json<SettingsPatch>,
) -> Result<Json<SettingsView>, (StatusCode, Json<serde_json::Value>)> {
    if !editable {
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

async fn chat(Room { app, editable, user }: Room, Json(req): Json<ChatRequest>) -> AppResult<Json<RoomState>> {
    let text = req.message.trim();
    if text.is_empty() {
        return Err(AppError::bad_request("пустое сообщение"));
    }
    if text.chars().count() > 4000 {
        return Err(AppError::bad_request("сообщение длиннее 4000 символов"));
    }
    let _turn = app.turn_lock.lock().await;
    let node_id = match req.node_id.as_deref().map(mermaid::safe_id) {
        Some(id) if db::node_exists(&app.db, &id).await? => Some(id),
        _ => None,
    };
    let msg = db::insert_message_full(&app.db, USER_ID, None, text, None, None, node_id.as_deref()).await?;
    app.emit(RoomEvent::Message { message: msg });
    match node_id {
        Some(id) => run_round(&app, Trigger::Node { id, mode: "discuss".into() }).await?,
        None => run_round(&app, Trigger::UserMessage).await?,
    }
    Ok(Json(build_state(&app, editable, user.as_ref()).await?))
}

async fn tick(Room { app, editable, user }: Room) -> AppResult<Json<RoomState>> {
    let _turn = app.turn_lock.lock().await;
    let alive = db::list_agents(&app.db).await?.into_iter().filter(|a| a.is_alive()).count();
    if alive == 0 {
        return Err(AppError::bad_request("в комнате никого не осталось — нажми «Новая комната»"));
    }
    run_round(&app, Trigger::Tick).await?;
    Ok(Json(build_state(&app, editable, user.as_ref()).await?))
}

/// Director → (1–2) agents, sequentially, so the second can answer the first.
async fn run_round(app: &AppState, trigger: Trigger) -> anyhow::Result<()> {
    let alive_before = alive_ids(app).await?;
    run_round_from(app, trigger, alive_before).await
}

// ───────────────────────────── Canvas: nodes, edges, notes ─────────────────────────────

async fn emit_graph(app: &AppState) -> anyhow::Result<()> {
    let graph = Graph { nodes: db::list_nodes(&app.db).await?, edges: db::list_edges(&app.db).await? };
    let mermaid = mermaid::build_graph(&graph.nodes, &graph.edges);
    app.emit(RoomEvent::Graph { graph, mermaid });
    Ok(())
}

async fn create_node(Room { app, editable, user }: Room, Json(req): Json<CreateNodeRequest>) -> AppResult<Json<RoomState>> {
    let label = mermaid::sanitize_label(req.label.trim());
    if label == "…" {
        return Err(AppError::bad_request("пустой заголовок"));
    }
    let mut id = mermaid::safe_id(&label);
    if db::node_exists(&app.db, &id).await? {
        id = format!("{id}_{}", chrono::Utc::now().timestamp() % 100_000);
    }
    db::upsert_node_full(
        &app.db,
        &id,
        &label,
        mermaid::normalize_node_type(req.node_type.as_deref().unwrap_or("concept")),
        USER_ID,
        req.body.as_deref(),
        req.x,
        req.y,
    )
    .await?;
    db::place_unpositioned_nodes(&app.db).await?;
    emit_graph(&app).await?;
    Ok(Json(build_state(&app, editable, user.as_ref()).await?))
}

async fn patch_node(Room { app, .. }: Room, Path(id): Path<String>, Json(req): Json<PatchNodeRequest>) -> AppResult<StatusCode> {
    let id = mermaid::safe_id(&id);
    if !db::node_exists(&app.db, &id).await? {
        return Err(AppError::bad_request("нет такого узла"));
    }
    if let (Some(x), Some(y)) = (req.x, req.y) {
        db::set_node_position(&app.db, &id, x, y).await?;
    }
    let label = req.label.as_deref().map(|l| mermaid::sanitize_label(l.trim())).filter(|l| l != "…");
    let body = req.body.as_deref().map(|b| mermaid::truncate_chars(b.trim(), 4000));
    let nt = req.node_type.as_deref().map(mermaid::normalize_node_type);
    if label.is_some() || body.is_some() || nt.is_some() || req.pinned.is_some() {
        db::update_node_fields(&app.db, &id, label.as_deref(), body.as_deref(), nt, req.pinned).await?;
        emit_graph(&app).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_node(Room { app, .. }: Room, Path(id): Path<String>) -> AppResult<StatusCode> {
    let id = mermaid::safe_id(&id);
    if !db::delete_node(&app.db, &id).await? {
        return Err(AppError::bad_request("нет такого узла"));
    }
    emit_graph(&app).await?;
    app.emit(RoomEvent::Notes { notes: db::list_notes(&app.db, 400).await? });
    Ok(StatusCode::NO_CONTENT)
}

async fn create_edge(Room { app, .. }: Room, Json(req): Json<CreateEdgeRequest>) -> AppResult<StatusCode> {
    let a = mermaid::safe_id(&req.source);
    let b = mermaid::safe_id(&req.target);
    if a == b || !db::node_exists(&app.db, &a).await? || !db::node_exists(&app.db, &b).await? {
        return Err(AppError::bad_request("связь должна соединять два разных существующих узла"));
    }
    let label = req.label.as_deref().map(mermaid::sanitize_label).filter(|l| l != "…").unwrap_or_default();
    db::insert_edge(&app.db, &a, &b, &mermaid::truncate_chars(&label, 30), USER_ID).await?;
    emit_graph(&app).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_edge(Room { app, .. }: Room, Path(id): Path<i64>) -> AppResult<StatusCode> {
    if !db::delete_edge(&app.db, id).await? {
        return Err(AppError::bad_request("нет такой связи"));
    }
    emit_graph(&app).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn add_note(Room { app, .. }: Room, Path(id): Path<String>, Json(req): Json<NoteRequest>) -> AppResult<StatusCode> {
    let id = mermaid::safe_id(&id);
    let text = req.text.trim();
    if text.is_empty() || !db::node_exists(&app.db, &id).await? {
        return Err(AppError::bad_request("пустая пометка или нет узла"));
    }
    db::insert_note(&app.db, &id, USER_ID, &mermaid::truncate_chars(text, 600)).await?;
    app.emit(RoomEvent::Notes { notes: db::list_notes(&app.db, 400).await? });
    Ok(StatusCode::NO_CONTENT)
}

/// The player pokes a node: the Triad discusses / develops / disputes it.
async fn ask_node(Room { app, editable, user }: Room, Path(id): Path<String>, Json(req): Json<AskNodeRequest>) -> AppResult<Json<RoomState>> {
    let id = mermaid::safe_id(&id);
    let Some(node) = db::get_node(&app.db, &id).await? else {
        return Err(AppError::bad_request("нет такого узла"));
    };
    let mode = match req.mode.as_deref().map(str::trim) {
        Some("develop") | Some("развить") => "develop",
        Some("dispute") | Some("оспорить") => "dispute",
        _ => "discuss",
    };
    let _turn = app.turn_lock.lock().await;
    let text = match req.message.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => format!("[узел «{}»] {t}", node.label),
        None => match mode {
            "develop" => format!("Развейте узел «{}».", node.label),
            "dispute" => format!("Оспорьте узел «{}».", node.label),
            _ => format!("Что вы думаете об узле «{}»?", node.label),
        },
    };
    let msg = db::insert_message_full(&app.db, USER_ID, None, &text, None, None, Some(&id)).await?;
    app.emit(RoomEvent::Message { message: msg });
    run_round(&app, Trigger::Node { id, mode: mode.into() }).await?;
    Ok(Json(build_state(&app, editable, user.as_ref()).await?))
}

/// Whole canvas as Markdown: notes with bodies, agents' annotations and links.
async fn export_markdown(Room { app, .. }: Room) -> AppResult<Response> {
    let agents = db::list_agents(&app.db).await?;
    let nodes = db::list_nodes(&app.db).await?;
    let edges = db::list_edges(&app.db).await?;
    let notes = db::list_notes(&app.db, 5000).await?;
    let name = |id: &str| prompts::sender_name(id, &agents);
    let mut out = String::from("# Triad Canvas — доска\n\n");
    out.push_str(&format!("_{} узлов, {} связей, экспорт {}_\n\n", nodes.len(), edges.len(), contracts::now_rfc3339()));
    for n in &nodes {
        out.push_str(&format!("## {}\n\n", n.label));
        out.push_str(&format!("`{}` · тип: {} · автор: {}{}\n\n", n.id, n.node_type, name(&n.created_by), if n.pinned != 0 { " · 📌" } else { "" }));
        if !n.body.trim().is_empty() {
            out.push_str(n.body.trim());
            out.push_str("\n\n");
        }
        let mine: Vec<&_> = notes.iter().filter(|x| x.node_id == n.id).collect();
        if !mine.is_empty() {
            out.push_str("**Пометки:**\n\n");
            for x in mine {
                out.push_str(&format!("- **{}**: {}\n", name(&x.author), x.text));
            }
            out.push('\n');
        }
        let links: Vec<String> = edges
            .iter()
            .filter(|e| e.source_id == n.id || e.target_id == n.id)
            .map(|e| {
                let other = if e.source_id == n.id { &e.target_id } else { &e.source_id };
                let other_label = nodes.iter().find(|x| &x.id == other).map(|x| x.label.as_str()).unwrap_or(other.as_str());
                if e.source_id == n.id {
                    format!("→ {} {}", if e.relation_label.is_empty() { "".to_string() } else { format!("_{}_", e.relation_label) }, other_label)
                } else {
                    format!("← {} {}", if e.relation_label.is_empty() { "".to_string() } else { format!("_{}_", e.relation_label) }, other_label)
                }
            })
            .collect();
        if !links.is_empty() {
            out.push_str("**Связи:** ");
            out.push_str(&links.join("; "));
            out.push_str("\n\n");
        }
    }
    Ok((
        [
            (header::CONTENT_TYPE, "text/markdown; charset=utf-8"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"triad-canvas.md\""),
        ],
        out,
    )
        .into_response())
}

async fn alive_ids(app: &AppState) -> anyhow::Result<Vec<String>> {
    Ok(db::list_agents(&app.db).await?.into_iter().filter(|a| a.is_alive()).map(|a| a.id).collect())
}

/// `alive_before` lets a caller that already changed the room (e.g. resolved a contract)
/// still give last words to whoever died from it.
async fn run_round_from(app: &AppState, trigger: Trigger, alive_before: Vec<String>) -> anyhow::Result<()> {
    // Deadlines bite before anyone speaks, so the Director sees the failure as an event.
    contracts::expire_overdue(app).await?;

    app.emit(RoomEvent::Status {
        stage: "director".into(),
        detail: "Режиссёр читает комнату и раздаёт скрытые цели…".into(),
    });
    let entry = director::run_director(app, &trigger).await?;
    let mood = entry.output.room_mood.clone();
    let agents = db::list_agents(&app.db).await?;
    let focus_node = match &trigger {
        Trigger::Node { id, .. } => Some(id.clone()),
        _ => None,
    };
    let mut scene_so_far: Vec<(String, String)> = Vec::new();
    for beat in &entry.output.scene {
        let name = prompts::sender_name(&beat.speaker, &agents);
        app.emit(RoomEvent::Status {
            stage: "agent".into(),
            detail: format!("{name} формулирует…"),
        });
        let plan = agent_runner::TurnPlan {
            beat: Some(beat),
            scene_so_far: scene_so_far.clone(),
            room_mood: mood.as_deref(),
            focus_node: focus_node.as_deref(),
        };
        match agent_runner::run_agent(app, &beat.speaker, plan).await {
            Ok(m) => scene_so_far.push((name, m.content)),
            Err(e) => tracing::warn!(agent = %beat.speaker, error = %e, "agent turn skipped"),
        }
    }
    // Anyone who died this round gets their last words.
    let agents = db::list_agents(&app.db).await?;
    for a in agents.iter().filter(|a| !a.is_alive() && alive_before.contains(&a.id)) {
        app.emit(RoomEvent::Status { stage: "agent".into(), detail: format!("{} … последние слова", a.name) });
        if let Err(e) = agent_runner::run_last_words(app, &a.id).await {
            tracing::warn!(agent = %a.id, error = %e, "last words skipped");
        }
    }
    if let Err(e) = memory::maybe_consolidate(app).await {
        tracing::warn!(error = %e, "memory scheduling failed");
    }
    app.emit(RoomEvent::Done);
    Ok(())
}

async fn resolve_contract(
    Room { app, editable, user }: Room,
    Path(id): Path<i64>,
    Json(req): Json<ResolveContractRequest>,
) -> AppResult<Json<RoomState>> {
    let status = req.status.trim().to_lowercase();
    if !matches!(status.as_str(), "done" | "failed" | "cancelled") {
        return Err(AppError::bad_request("status must be done | failed | cancelled"));
    }
    let _turn = app.turn_lock.lock().await;
    let alive_before = alive_ids(&app).await?;
    let Some(c) = contracts::resolve(&app, id, &status, req.note.as_deref(), "user").await? else {
        return Err(AppError::bad_request("нет такого слова"));
    };
    if c.status == status && status != "cancelled" {
        // Let the room react in character (guarantor first); the dead get their last words.
        run_round_from(&app, Trigger::Event, alive_before).await?;
    }
    Ok(Json(build_state(&app, editable, user.as_ref()).await?))
}

async fn synthesize_schema(Room { app, editable, user }: Room) -> AppResult<Json<RoomState>> {
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
        presence_penalty: None,
        frequency_penalty: None,
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
    Ok(Json(build_state(&app, editable, user.as_ref()).await?))
}

async fn reset(Room { app, editable, user }: Room) -> AppResult<Json<RoomState>> {
    let _turn = app.turn_lock.lock().await;
    db::reset_room(&app.db).await?;
    let state = build_state(&app, editable, user.as_ref()).await?;
    app.emit(RoomEvent::Agents { agents: state.agents.clone() });
    app.emit(RoomEvent::Graph {
        graph: state.graph.clone(),
        mermaid: state.graph_mermaid.clone(),
    });
    app.emit(RoomEvent::Contracts { contracts: state.contracts.clone() });
    app.emit(RoomEvent::Notes { notes: state.node_notes.clone() });
    app.emit(RoomEvent::Relations { relations: state.relations.clone() });
    app.emit(RoomEvent::Memories { memories: state.memories.clone() });
    app.emit(RoomEvent::Done);
    Ok(Json(state))
}

/// Server-Sent Events: live progress while a round runs (works through any WebView/proxy).
async fn events(Room { app, .. }: Room) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
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
