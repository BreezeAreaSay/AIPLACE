//! Axum routes: state, chat, tick, schema synthesis, reset, SSE event stream.

use std::{convert::Infallible, net::SocketAddr};

use axum::{
    async_trait,
    extract::{ConnectInfo, DefaultBodyLimit, FromRequestParts, Multipart, Path, Query, State},
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
        AskNodeRequest, ChatRequest, CreateEdgeRequest, CreateNodeRequest, CreateTypeRequest, Graph, NoteRequest, PatchNodeRequest,
        AgentCreateRequest, AgentPatchRequest, ImportReport, ImportRequest, ResolveContractRequest, RoomEvent, UpdateTypeRequest, RoomState, SchemaDoc, SettingsPatch, SettingsView, USER_ID,
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
        let requested = parts
            .headers
            .get("x-triad-room")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
            .or_else(|| parts.uri.query().and_then(|q| form_urlencoded::parse(q.as_bytes()).find(|(k, _)| k == "room").map(|(_, v)| v.to_string())))
            .map(|r| r.trim().to_string())
            .filter(|r| !r.is_empty());
        let room_id = match (&user, requested) {
            (Some(u), _) => telegram::room_id_for(u),
            (None, Some(r)) if r == LOCAL_ROOM || r.starts_with("yard_") => r,
            _ => LOCAL_ROOM.to_string(),
        };
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
        .route("/api/models", get(list_models))
        .route("/api/contracts/:id/resolve", post(resolve_contract))
        .route("/api/nodes", post(create_node))
        .route("/api/nodes/:id", axum::routing::patch(patch_node).delete(delete_node))
        .route("/api/nodes/:id/ask", post(ask_node))
        .route("/api/nodes/:id/notes", post(add_note))
        .route("/api/edges", post(create_edge))
        .route("/api/types", get(list_types).post(create_type))
        .route("/api/nodes/:id/backlinks", get(node_backlinks))
        .route("/api/search", get(search))
        .route("/api/seen", post(mark_seen))
        .route("/api/tools", get(list_tools))
        .route("/mcp", post(mcp_rpc).get(mcp_info))
        .route("/api/edges/:id", delete(delete_edge))
        .route("/api/export.md", get(export_markdown))
        .route("/api/nodes/:id/export.md", get(export_node_markdown))
        .route("/api/import", post(import_markdown))
        .route("/api/nodes/:id/files", post(upload_to_node))
        .route("/api/files", post(upload_files))
        .route("/api/files/:id", get(serve_file).delete(delete_file))
        .route("/api/files/:id/text", get(file_text))
        .route("/api/types/:id", axum::routing::patch(update_type).delete(delete_type))
        .route("/api/lookup", get(lookup))
        .route("/api/nodes/:id/autofill", post(autofill_node))
        .route("/api/nodes/:id/github", post(github_into_node))
        .route("/api/weather", get(weather))
        .route("/api/calendar.ics", get(calendar_ics))
        .route("/api/calendar", get(calendar_upcoming))
        .route("/api/tts", post(tts))
        .route("/api/stt", post(stt))
        .route("/api/feeds/poll", post(poll_feeds))
        .route("/api/roles", get(list_roles))
        .route("/api/messages", get(older_messages))
        .route("/api/messages/search", get(search_messages))
        .route("/api/messages/:id", axum::routing::patch(edit_message).delete(delete_message))
        .route("/api/llm/log", get(llm_log))
        .route("/api/organism", get(organism_state))
        .route("/api/organism/audit", post(organism_audit))
        .route("/api/proposals", get(list_proposals))
        .route("/api/proposals/:id/accept", post(accept_proposal))
        .route("/api/proposals/:id/reject", post(reject_proposal))
        .route("/api/messages/:id/react", post(react_message))
        .route("/api/dm/:agent", get(dm_history).post(dm_send))
        .route("/api/dm", get(dm_overview))
        .route("/api/rooms", get(rooms_list).post(rooms_create))
        .route("/api/rooms/:id", axum::routing::patch(rooms_rename).delete(rooms_delete))
        .route("/api/snapshots", get(snapshots_list).post(snapshots_create))
        .route("/api/snapshots/:name/restore", post(snapshots_restore))
        .route("/api/snapshots/:name", delete(snapshots_delete))
        .route("/api/organism/tend", post(organism_tend))
        .route("/api/organism/chronicle", post(organism_chronicle))
        .route("/api/backup.zip", get(backup_zip))
        .route("/api/restore", post(restore_upload).delete(restore_cancel))
        .route("/api/update/check", post(update_check))
        .route("/api/update/run", post(update_run))
        .route("/api/update/status", get(update_status).delete(update_ack))
        .route("/api/agents", post(create_agent))
        .route("/api/agents/:id", axum::routing::patch(patch_agent).delete(retire_agent))
        .layer(DefaultBodyLimit::max(60 * 1024 * 1024))
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
        "usage": app.llm.usage(),
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
    let s_feat = s.clone();
    Ok(RoomState {
        agents: db::list_agents(&app.db).await?,
        messages: db::with_mentions(&app.db, db::recent_messages(&app.db, 80).await?).await?,
        graph: Graph { nodes, edges },
        types: db::list_types(&app.db).await?,
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
        last_seen_message_id: db::meta_get(&app.db, "last_seen").await?.and_then(|v| v.parse().ok()).unwrap_or(0),
        usage: app.llm.usage(),
        heartbeat_minutes: app.cfg.heartbeat_minutes,
        attachments: db::list_attachments(&app.db).await?,
        source_update: app.source_update.read().await.clone(),
        message_count: db::message_count(&app.db).await?,
        restore_pending: crate::backup::pending_info(&app.room_id),
        repo_dir: crate::selfupdate::find_repo_dir().map(|p| p.display().to_string()),
        proposals: db::list_proposals(&app.db, Some("pending"), 30).await?,
        vision: !s_feat.mock_mode() && app.catalog.supports_vision(s_feat.api_key.as_deref(), &s_feat.director_model).await,
        exe_path: std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default(),
        run_mode: crate::selfupdate::mode().into(),
        player_name: s_feat.get("PLAYER_NAME").map(str::to_string),
        update_status: crate::selfupdate::status(),
        world: crate::integrations::world_line(app).await,
        features: crate::integrations::features(&s_feat),
    })
}

async fn list_tools(Room { app, .. }: Room) -> Json<Vec<crate::tools::ToolDef>> {
    Json(crate::tools::definitions_enabled(&app.llm_settings().await))
}

async fn mcp_info() -> Json<serde_json::Value> {
    Json(json!({ "name": "triad-canvas", "transport": "streamable-http (JSON-RPC over POST)", "version": crate::update::VERSION, "hint": "POST JSON-RPC 2.0: initialize, tools/list, tools/call. Auth: loopback, or Authorization: Bearer <MCP_TOKEN>." }))
}

/// Minimal MCP server (JSON-RPC 2.0 over HTTP POST): lets Claude Desktop / Cursor / any agent
/// read and write this room's library with the same tools the inhabitants use.
async fn mcp_rpc(
    State(app): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<serde_json::Value>,
) -> Response {
    let loopback = peer.ip().is_loopback() && !is_forwarded(&headers);
    let token_ok = match &app.cfg.mcp_token {
        Some(t) => headers.get("authorization").and_then(|v| v.to_str().ok()).map(|v| v.trim() == format!("Bearer {t}")).unwrap_or(false),
        None => false,
    };
    if !loopback && !token_ok {
        return (StatusCode::UNAUTHORIZED, Json(json!({ "jsonrpc": "2.0", "id": req.get("id"), "error": { "code": -32001, "message": "unauthorized: set MCP_TOKEN and send Authorization: Bearer" } }))).into_response();
    }
    // External MCP clients act on the local room (desktop) — the same one the player sees.
    let room = match app.for_room(crate::rooms::LOCAL_ROOM).await {
        Ok(r) => r,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    };
    let id = req.get("id").cloned().unwrap_or(serde_json::Value::Null);
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(json!({}));
    let result = match method {
        "initialize" => json!({
            "protocolVersion": params.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or("2025-03-26"),
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "triad-canvas", "version": crate::update::VERSION },
            "instructions": crate::tools::system_brief(db::list_agents(&app.db).await.map(|a| a.iter().filter(|x| x.is_alive()).count().saturating_sub(1)).unwrap_or(2)),
        }),
        "notifications/initialized" | "initialized" => return StatusCode::ACCEPTED.into_response(),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": crate::tools::definitions_enabled(&app.llm_settings().await) }),
        "tools/call" => {
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match crate::tools::call(&room, "mcp", name, &args).await {
                Ok((v, rec)) => {
                    if rec.is_some() {
                        let graph = Graph { nodes: db::list_nodes(&room.db).await.unwrap_or_default(), edges: db::list_edges(&room.db).await.unwrap_or_default() };
                        let mermaid = mermaid::build_graph(&graph.nodes, &graph.edges);
                        room.emit(RoomEvent::Graph { graph, mermaid });
                        if let Ok(notes) = db::list_notes(&room.db, 400).await {
                            room.emit(RoomEvent::Notes { notes });
                        }
                    }
                    let is_err = v.get("error").is_some();
                    json!({ "content": [{ "type": "text", "text": v.to_string() }], "isError": is_err })
                }
                Err(e) => json!({ "content": [{ "type": "text", "text": e.to_string() }], "isError": true }),
            }
        }
        other => {
            return Json(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("method not found: {other}") } })).into_response();
        }
    };
    Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })).into_response()
}

#[derive(serde::Deserialize)]
pub struct SeenRequest {
    pub message_id: i64,
}

/// The client saw everything up to `message_id`; also resets the away-backoff of the heartbeat.
async fn mark_seen(Room { app, .. }: Room, Json(req): Json<SeenRequest>) -> AppResult<StatusCode> {
    let prev: i64 = db::meta_get(&app.db, "last_seen").await?.and_then(|v| v.parse().ok()).unwrap_or(0);
    if req.message_id > prev {
        db::meta_set(&app.db, "last_seen", &req.message_id.to_string()).await?;
    }
    crate::heartbeat::player_is_back(&app).await?;
    Ok(StatusCode::NO_CONTENT)
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
        model: s.model.clone(),
        director_model: s.director_model.clone(),
        editable,
        config_path: crate::config::config_path().display().to_string(),
        version: crate::update::VERSION.into(),
        update: app.update.read().await.clone(),
        ui_embedded: crate::web::is_embedded(),
        director_mode: app.cfg.director_mode.clone(),
        agent_mode: app.cfg.agent_mode.clone(),
        agent_tools: app.cfg.agent_tools.clone(),
        integrations: crate::config::INTEGRATION_KEYS
            .iter()
            .map(|(k, secret, hint)| {
                let v = s.integrations.get(*k).cloned().unwrap_or_default();
                crate::models::IntegrationView {
                    key: k.to_string(),
                    value: if *secret && !v.is_empty() { mask(&v) } else { v.clone() },
                    secret: *secret,
                    set: !v.is_empty(),
                    hint: hint.to_string(),
                }
            })
            .collect(),
        features: crate::integrations::features(&s),
    }
}

fn mask(v: &str) -> String {
    let n = v.chars().count();
    if n <= 8 {
        return "•".repeat(n);
    }
    format!("{}…{}", v.chars().take(4).collect::<String>(), v.chars().skip(n - 3).collect::<String>())
}

async fn list_models(State(app): State<AppState>) -> Json<Vec<crate::catalog::ModelInfo>> {
    let key = app.llm_settings().await.api_key;
    // Offline / blocked network must not break the settings UI: an empty catalog still lets you type an id.
    Json(app.catalog.models(key.as_deref()).await.unwrap_or_default())
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
    if let Some(map) = patch.integrations {
        for (k, v) in map {
            if !crate::config::INTEGRATION_KEYS.iter().any(|(key, _, _)| *key == k) {
                continue;
            }
            let v = v.trim().to_string();
            // a masked value echoed back from the UI means "unchanged"
            if v.contains('•') || (v.contains('…') && v.chars().count() < 12) {
                continue;
            }
            if v.is_empty() {
                s.integrations.remove(&k);
            } else {
                s.integrations.insert(k, v);
            }
        }
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
    let node_id = match req.node_id.as_deref().map(mermaid::safe_id) {
        Some(id) if db::node_exists(&app.db, &id).await? => Some(id),
        _ => None,
    };
    // The line lands immediately — even mid-round, so the next beats see it in the transcript.
    let reply_to = match req.reply_to {
        Some(id) => db::get_message(&app.db, id).await?.map(|_| id),
        None => None,
    };
    let msg = db::insert_message_in(&app.db, "", USER_ID, None, text, None, None, node_id.as_deref(), reply_to).await?;
    app.emit(RoomEvent::Message { message: msg.clone() });
    crate::heartbeat::player_is_back(&app).await?;
    let turn = match app.turn_lock.try_lock() {
        Ok(g) => g,
        Err(_) => {
            // A round is running: queue a follow-up round instead of blocking the client.
            app.pending.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Ok(Json(build_state(&app, editable, user.as_ref()).await?));
        }
    };
    let _turn = turn;
    // Links in the line become bookmarks before anyone speaks, so the room discusses content.
    let urls = crate::intake::find_urls(text);
    let mut digest: Vec<String> = Vec::new();
    for url in urls {
        app.emit(RoomEvent::Status { stage: "intake".into(), detail: format!("Читаю {}…", mermaid::truncate_chars(&url, 60)) });
        match crate::intake::ingest_url(&app, &url).await {
            Ok(i) => {
                db::add_mention(&app.db, msg.id, &i.object_id).await?;
                digest.push(format!("- [Закладка] «{}» ({url}): {}", i.title, mermaid::truncate_chars(&i.summary, 900)));
            }
            Err(e) => tracing::warn!(error = %e, %url, "intake failed"),
        }
    }
    if !digest.is_empty() {
        emit_graph(&app).await?;
        app.emit(RoomEvent::Message { message: db::with_mentions(&app.db, vec![msg.clone()]).await?.remove(0) });
    }
    let attachments = if digest.is_empty() { None } else { Some(digest.join("\n")) };
    match node_id {
        Some(id) => run_round_with(&app, Trigger::Node { id, mode: "discuss".into() }, attachments).await?,
        None => run_round_with(&app, Trigger::UserMessage, attachments).await?,
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
pub async fn run_round(app: &AppState, trigger: Trigger) -> anyhow::Result<()> {
    let alive_before = alive_ids(app).await?;
    run_round_from(app, trigger, alive_before, None).await
}

/// Like `run_round`, with extra context (e.g. digests of links the player just pasted).
pub async fn run_round_with(app: &AppState, trigger: Trigger, attachments: Option<String>) -> anyhow::Result<()> {
    let alive_before = alive_ids(app).await?;
    run_round_from(app, trigger, alive_before, attachments).await
}

// ───────────────────────────── Canvas: nodes, edges, notes ─────────────────────────────

pub async fn emit_graph_pub(app: &AppState) -> anyhow::Result<()> {
    emit_graph(app).await
}

/// Digest freshly uploaded readable files in the background (one small LLM call each).
fn spawn_digest(app: &AppState, node_id: &str) {
    let app = app.clone();
    let node_id = node_id.to_string();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let s = app.llm_settings().await;
        if s.mock_mode() && !app.cfg.heartbeat_in_mock {
            return;
        }
        if !app.llm.budget_left() {
            return;
        }
        let _turn = app.turn_lock.lock().await;
        let Ok(agents) = db::list_agents(&app.db).await else { return };
        let Ok(Some(node)) = db::get_node(&app.db, &node_id).await else { return };
        let Some(agent) = crate::organism::pick_carer(&agents, &node, "digest") else { return };
        match crate::organism::tend(&app, &agent.id, &node_id, "digest").await {
            Ok((recs, _)) => tracing::info!(node = %node_id, agent = %agent.id, actions = recs.len(), "uploaded file digested"),
            Err(e) => tracing::warn!(node = %node_id, error = %e, "digest failed"),
        }
        app.emit(RoomEvent::Done);
    });
}

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
    let type_id = match req.type_id.as_deref().map(mermaid::safe_id) {
        Some(t) if db::type_exists(&app.db, &t).await? => t,
        _ => crate::types_seed::legacy_type(req.node_type.as_deref().unwrap_or("note")).to_string(),
    };
    // An empty body starts from the type's template (if it has one).
    let template: Option<String> = db::list_types(&app.db).await?.into_iter().find(|t| t.id == type_id).and_then(|t| t.schema.0.get("template").and_then(|x| x.as_str()).map(str::to_string)).filter(|t| !t.trim().is_empty());
    let body = req.body.as_deref().filter(|b| !b.trim().is_empty()).map(str::to_string).or(template);
    db::upsert_object(&app.db, &id, &label, &type_id, USER_ID, body.as_deref(), req.props.as_ref(), req.x, req.y).await?;
    db::place_unpositioned_nodes(&app.db).await?;
    emit_graph(&app).await?;
    Ok(Json(build_state(&app, editable, user.as_ref()).await?))
}

async fn list_types(Room { app, .. }: Room) -> AppResult<Json<Vec<crate::models::ObjectType>>> {
    Ok(Json(db::list_types(&app.db).await?))
}

async fn create_type(Room { app, .. }: Room, Json(req): Json<CreateTypeRequest>) -> AppResult<Json<Vec<crate::models::ObjectType>>> {
    let name = mermaid::truncate_chars(req.name.trim(), 40);
    if name.chars().count() < 2 {
        return Err(AppError::bad_request("имя типа слишком короткое"));
    }
    let id = mermaid::safe_id(&name);
    if db::type_exists(&app.db, &id).await? {
        return Err(AppError::bad_request("такой тип уже есть"));
    }
    db::insert_type(&app.db, &id, &name, req.icon.as_deref().unwrap_or("◇"), req.color.as_deref().unwrap_or("#9a9ca8"), &serde_json::json!({ "props": req.props }), USER_ID).await?;
    let types = db::list_types(&app.db).await?;
    app.emit(RoomEvent::Types { types: types.clone() });
    Ok(Json(types))
}

async fn node_backlinks(Room { app, .. }: Room, Path(id): Path<String>) -> AppResult<Json<Vec<crate::models::Message>>> {
    let id = mermaid::safe_id(&id);
    Ok(Json(db::with_mentions(&app.db, db::messages_mentioning(&app.db, &id, 50).await?).await?))
}

#[derive(serde::Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    q: String,
    #[serde(default, rename = "type")]
    type_id: Option<String>,
}

async fn search(Room { app, .. }: Room, axum::extract::Query(q): axum::extract::Query<SearchQuery>) -> AppResult<Json<Vec<crate::models::GraphNode>>> {
    Ok(Json(db::search_objects(&app.db, &q.q, q.type_id.as_deref().filter(|t| !t.is_empty()), 30).await?))
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
    let body = req.body.as_deref().map(|b| mermaid::truncate_chars(b.trim(), 60_000));
    let nt = req.node_type.as_deref().map(mermaid::normalize_node_type);
    let mut changed = false;
    if label.is_some() || body.is_some() || nt.is_some() || req.pinned.is_some() {
        db::update_node_fields(&app.db, &id, label.as_deref(), body.as_deref(), nt, req.pinned).await?;
        changed = true;
    }
    let type_id = match req.type_id.as_deref().map(mermaid::safe_id) {
        Some(t) if db::type_exists(&app.db, &t).await? => Some(t),
        Some(_) => return Err(AppError::bad_request("нет такого типа")),
        None => None,
    };
    if type_id.is_some() || req.props.is_some() || req.archived.is_some() {
        db::update_object_meta(&app.db, &id, type_id.as_deref(), req.props.as_ref(), req.archived).await?;
        changed = true;
    }
    if changed {
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
    let kind = req.kind.as_deref().map(|k| k.trim().to_lowercase()).filter(|k| ["related", "belongs_to", "about", "blocks", "made_by", "assigned_to", "contradicts", "mentions"].contains(&k.as_str())).unwrap_or_else(|| "related".into());
    db::insert_relation(&app.db, &a, &b, &kind, &mermaid::truncate_chars(&label, 30), USER_ID, 1.0).await?;
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
/// One object as a standalone `.md` (frontmatter + body + notes + wikilinked relations).
async fn export_node_markdown(Room { app, .. }: Room, Path(id): Path<String>) -> AppResult<Response> {
    let id = mermaid::safe_id(&id);
    let Some(node) = db::get_node(&app.db, &id).await? else { return Err(AppError::bad_request("нет такого узла")) };
    let agents = db::list_agents(&app.db).await?;
    let nodes = db::list_nodes(&app.db).await?;
    let edges = db::list_edges(&app.db).await?;
    let notes = db::list_notes(&app.db, 5000).await?;
    let types = db::list_types(&app.db).await?;
    let ty = types.iter().find(|t| t.id == node.type_id);
    let name = |id: &str| prompts::sender_name(id, &agents);
    let md = crate::markdown::render_note(&node, ty, &notes, &edges, &nodes, &name);
    let fname: String = node.label.chars().filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_').take(60).collect();
    Ok((
        [
            (header::CONTENT_TYPE, "text/markdown; charset=utf-8".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename*=UTF-8''{}.md", urlencoding(&fname.trim().replace(' ', "_")))),
        ],
        md,
    )
        .into_response())
}

fn urlencoding(s: &str) -> String {
    form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// Drop a folder of `.md` files (Obsidian / Anytype export / plain notes) into the library.
/// Frontmatter `type:` picks the type (id or name), other known keys become props, `[[wikilinks]]`
/// between imported or existing objects become `related` edges.
async fn import_markdown(Room { app, .. }: Room, Json(req): Json<ImportRequest>) -> AppResult<Json<ImportReport>> {
    if req.files.is_empty() || req.files.len() > 500 {
        return Err(AppError::bad_request("нужно от 1 до 500 файлов"));
    }
    let types = db::list_types(&app.db).await?;
    let mut report = ImportReport { imported: 0, updated: 0, linked: 0, ids: vec![] };
    let mut pending_links: Vec<(String, Vec<String>)> = vec![];
    for f in &req.files {
        if f.content.chars().count() > 200_000 {
            continue;
        }
        let stem = f.name.rsplit('/').next().unwrap_or(&f.name).trim_end_matches(".md").trim_end_matches(".txt").trim_end_matches(".markdown");
        let doc = crate::markdown::parse(&f.content, stem);
        let label = mermaid::sanitize_label(&doc.title);
        if label == "…" {
            continue;
        }
        let type_id = doc
            .front
            .get("type")
            .and_then(|t| t.as_str())
            .map(|t| t.trim().to_lowercase())
            .and_then(|t| types.iter().find(|x| x.id == t || x.name.to_lowercase() == t).map(|x| x.id.clone()))
            .unwrap_or_else(|| "note".to_string());
        let schema_keys: Vec<String> = types.iter().find(|t| t.id == type_id).and_then(|t| t.schema.0.get("props")).and_then(|p| p.as_array()).map(|arr| arr.iter().filter_map(|p| p.get("key").and_then(|k| k.as_str()).map(str::to_string)).collect()).unwrap_or_default();
        let mut props = serde_json::Map::new();
        for (k, v) in &doc.front {
            if ["title", "type", "id", "author", "created", "updated", "pinned", "archived", "aliases"].contains(&k.as_str()) || v.is_null() {
                continue;
            }
            if schema_keys.iter().any(|s| s == k) || k == "tags" || k == "url" || k == "source" {
                props.insert(k.clone(), v.clone());
            }
        }
        let id = doc.front.get("id").and_then(|v| v.as_str()).map(mermaid::safe_id).filter(|s| !s.is_empty()).unwrap_or_else(|| mermaid::safe_id(&label));
        let existed = db::node_exists(&app.db, &id).await?;
        let body = mermaid::truncate_chars(&doc.body, 60_000);
        db::upsert_object(&app.db, &id, &label, &type_id, USER_ID, Some(&body), Some(&serde_json::Value::Object(props)), None, None).await?;
        if doc.front.get("pinned").and_then(|v| v.as_bool()) == Some(true) {
            db::update_node_fields(&app.db, &id, None, None, None, Some(true)).await?;
        }
        if doc.front.get("archived").and_then(|v| v.as_bool()) == Some(true) {
            db::update_object_meta(&app.db, &id, None, None, Some(true)).await?;
        }
        if existed {
            report.updated += 1;
        } else {
            report.imported += 1;
        }
        report.ids.push(id.clone());
        let links = crate::markdown::wikilinks(&doc.body);
        if !links.is_empty() {
            pending_links.push((id, links));
        }
    }
    // Resolve wikilinks against the whole library (imported + existing), by id or by title.
    let nodes = db::list_nodes(&app.db).await?;
    for (from, targets) in pending_links {
        for t in targets {
            let tid = mermaid::safe_id(&t);
            let found = nodes.iter().find(|n| n.id == tid || n.label.eq_ignore_ascii_case(&t) || n.label.to_lowercase() == t.to_lowercase());
            if let Some(n) = found {
                if n.id != from {
                    db::insert_relation(&app.db, &from, &n.id, "related", "", USER_ID, 1.0).await?;
                    report.linked += 1;
                }
            }
        }
    }
    db::place_unpositioned_nodes(&app.db).await?;
    emit_graph(&app).await?;
    Ok(Json(report))
}

// ───────────────────────────── Files ─────────────────────────────

struct Upload {
    name: String,
    mime: Option<String>,
    bytes: Vec<u8>,
}

async fn read_multipart(mut mp: Multipart) -> AppResult<(Vec<Upload>, std::collections::HashMap<String, String>)> {
    let mut files = vec![];
    let mut fields = std::collections::HashMap::new();
    while let Some(field) = mp.next_field().await.map_err(|e| AppError::bad_request(format!("multipart: {e}")))? {
        let fname = field.file_name().map(str::to_string);
        let key = field.name().unwrap_or("").to_string();
        let mime = field.content_type().map(str::to_string);
        match fname {
            Some(name) => {
                let bytes = field.bytes().await.map_err(|e| AppError::bad_request(format!("чтение файла: {e}")))?;
                files.push(Upload { name, mime, bytes: bytes.to_vec() });
            }
            None => {
                let v = field.text().await.unwrap_or_default();
                fields.insert(key, v);
            }
        }
    }
    Ok((files, fields))
}

async fn emit_attachments(app: &AppState) -> anyhow::Result<()> {
    app.emit(RoomEvent::Attachments { attachments: db::list_attachments(&app.db).await? });
    Ok(())
}

/// Attach files to an existing object.
async fn upload_to_node(Room { app, .. }: Room, Path(id): Path<String>, mp: Multipart) -> AppResult<Json<Vec<crate::models::Attachment>>> {
    let id = mermaid::safe_id(&id);
    if !db::node_exists(&app.db, &id).await? {
        return Err(AppError::bad_request("нет такого объекта"));
    }
    let (files, _) = read_multipart(mp).await?;
    if files.is_empty() {
        return Err(AppError::bad_request("нет файлов"));
    }
    let mut out = vec![];
    for f in files {
        out.push(crate::files::save(&app, &id, &f.name, f.mime.as_deref(), &f.bytes, USER_ID).await.map_err(|e| AppError::bad_request(format!("{e:#}")))?);
    }
    emit_attachments(&app).await?;
    emit_graph(&app).await?;
    if out.iter().any(|a| a.text_len > 0) {
        spawn_digest(&app, &id);
    }
    Ok(Json(out))
}

/// Upload files as new objects: `.md` → import, images → «Картинка», everything else → «Файл»
/// (or the type given in the `type` field). Returns the created object ids.
async fn upload_files(Room { app, .. }: Room, mp: Multipart) -> AppResult<Json<serde_json::Value>> {
    let (files, fields) = read_multipart(mp).await?;
    if files.is_empty() {
        return Err(AppError::bad_request("нет файлов"));
    }
    let forced_type = fields.get("type").map(|t| mermaid::safe_id(t)).filter(|t| !t.is_empty());
    let mut ids = vec![];
    let mut atts = vec![];
    for f in files {
        let mime = crate::files::guess_mime(&f.name, f.mime.as_deref());
        let stem = f.name.rsplit('/').next().unwrap_or(&f.name).rsplit_once('.').map(|(a, _)| a).unwrap_or(&f.name).replace(['_', '-'], " ");
        let label = mermaid::sanitize_label(stem.trim());
        let label = if label == "…" { "Файл".to_string() } else { label };
        let type_id = match &forced_type {
            Some(t) if db::type_exists(&app.db, t).await? => t.clone(),
            _ => if crate::files::is_image(&mime) { "image".into() } else { "file".into() },
        };
        let mut id = mermaid::safe_id(&label);
        if db::node_exists(&app.db, &id).await? {
            id = format!("{id}_{}", chrono::Utc::now().timestamp() % 100_000);
        }
        let kind = mime.split('/').next_back().unwrap_or("").to_uppercase();
        let props = if type_id == "file" { serde_json::json!({ "kind": kind }) } else { serde_json::json!({}) };
        db::upsert_object(&app.db, &id, &label, &type_id, USER_ID, None, Some(&props), None, None).await?;
        let a = crate::files::save(&app, &id, &f.name, f.mime.as_deref(), &f.bytes, USER_ID).await.map_err(|e| AppError::bad_request(format!("{e:#}")))?;
        // a readable document: first lines become the summary / body preview
        if a.text_len > 0 {
            let preview = mermaid::truncate_chars(&a.text.split_whitespace().collect::<Vec<_>>().join(" "), 400);
            db::update_object_meta(&app.db, &id, None, Some(&serde_json::json!({ "summary": preview })), None).await?;
        }
        if a.text_len > 0 {
            spawn_digest(&app, &id);
        }
        ids.push(id);
        atts.push(a);
    }
    db::place_unpositioned_nodes(&app.db).await?;
    emit_attachments(&app).await?;
    emit_graph(&app).await?;
    Ok(Json(json!({ "ids": ids, "attachments": atts })))
}

async fn serve_file(Room { app, .. }: Room, Path(id): Path<String>) -> AppResult<Response> {
    let Some(a) = db::get_attachment(&app.db, &id).await? else { return Err(AppError::bad_request("нет такого файла")) };
    let bytes = tokio::fs::read(&a.path).await.map_err(|e| AppError::bad_request(format!("файл пропал с диска: {e}")))?;
    let inline = crate::files::is_image(&a.mime) || a.mime == "application/pdf" || a.mime.starts_with("text/") || a.mime.starts_with("audio/") || a.mime.starts_with("video/");
    Ok((
        [
            (header::CONTENT_TYPE, a.mime.clone()),
            (header::CONTENT_DISPOSITION, format!("{}; filename*=UTF-8''{}", if inline { "inline" } else { "attachment" }, urlencoding(&a.name))),
            (header::CACHE_CONTROL, "private, max-age=86400".to_string()),
        ],
        bytes,
    )
        .into_response())
}

async fn file_text(Room { app, .. }: Room, Path(id): Path<String>) -> AppResult<Json<serde_json::Value>> {
    let Some(a) = db::get_attachment(&app.db, &id).await? else { return Err(AppError::bad_request("нет такого файла")) };
    Ok(Json(json!({ "id": a.id, "name": a.name, "chars": a.text_len, "text": a.text })))
}

async fn delete_file(Room { app, .. }: Room, Path(id): Path<String>) -> AppResult<StatusCode> {
    crate::files::remove(&app, &id).await?;
    emit_attachments(&app).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ───────────────────────────── Types ─────────────────────────────

fn clean_schema(v: &serde_json::Value) -> serde_json::Value {
    let props: Vec<serde_json::Value> = v
        .get("props")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    let key = mermaid::safe_id(p.get("key").and_then(|k| k.as_str()).unwrap_or("")).chars().take(32).collect::<String>();
                    if key.is_empty() {
                        return None;
                    }
                    let label = mermaid::truncate_chars(p.get("label").and_then(|l| l.as_str()).unwrap_or(&key).trim(), 40);
                    let kind = p.get("kind").and_then(|k| k.as_str()).unwrap_or("text");
                    let kind = if ["text", "longtext", "number", "checkbox", "date", "datetime", "select", "multiselect", "tags", "rating", "url", "email", "phone", "image", "relation"].contains(&kind) { kind } else { "text" };
                    let mut out = json!({ "key": key, "label": label, "kind": kind });
                    if let Some(opts) = p.get("options").and_then(|o| o.as_array()) {
                        out["options"] = json!(opts.iter().filter_map(|o| o.as_str()).map(|o| mermaid::truncate_chars(o.trim(), 40)).filter(|o| !o.is_empty()).take(30).collect::<Vec<_>>());
                    }
                    if let Some(to) = p.get("to").and_then(|t| t.as_str()).filter(|t| !t.is_empty()) {
                        out["to"] = json!(mermaid::safe_id(to));
                    }
                    if let Some(max) = p.get("max").and_then(|m| m.as_i64()) {
                        out["max"] = json!(max.clamp(2, 10));
                    }
                    Some(out)
                })
                .take(40)
                .collect()
        })
        .unwrap_or_default();
    let mut out = json!({ "props": props });
    if let Some(t) = v.get("template").and_then(|t| t.as_str()) {
        out["template"] = json!(mermaid::truncate_chars(t, 4000));
    }
    out
}

async fn update_type(Room { app, .. }: Room, Path(id): Path<String>, Json(req): Json<UpdateTypeRequest>) -> AppResult<Json<Vec<crate::models::ObjectType>>> {
    let id = mermaid::safe_id(&id);
    if !db::type_exists(&app.db, &id).await? {
        return Err(AppError::bad_request("нет такого типа"));
    }
    let name = req.name.as_deref().map(|n| mermaid::truncate_chars(n.trim(), 40)).filter(|n| n.chars().count() >= 2);
    let icon = req.icon.as_deref().map(|i| i.trim().chars().take(4).collect::<String>()).filter(|i| !i.is_empty());
    let color = req.color.as_deref().map(str::trim).filter(|c| c.starts_with('#') && (c.len() == 7 || c.len() == 4)).map(str::to_string);
    let schema = req.schema.as_ref().map(clean_schema);
    db::update_type(&app.db, &id, name.as_deref(), icon.as_deref(), color.as_deref(), schema.as_ref()).await?;
    let types = db::list_types(&app.db).await?;
    app.emit(RoomEvent::Types { types: types.clone() });
    Ok(Json(types))
}

async fn delete_type(Room { app, .. }: Room, Path(id): Path<String>) -> AppResult<Json<Vec<crate::models::ObjectType>>> {
    let id = mermaid::safe_id(&id);
    if ["note", "page", "task", "file", "image"].contains(&id.as_str()) {
        return Err(AppError::bad_request("этот тип нужен системе"));
    }
    db::delete_type(&app.db, &id).await?;
    let types = db::list_types(&app.db).await?;
    app.emit(RoomEvent::Types { types: types.clone() });
    emit_graph(&app).await?;
    Ok(Json(types))
}

// ───────────────────────────── World ─────────────────────────────

#[derive(serde::Deserialize)]
struct LookupQuery {
    kind: Option<String>,
    q: String,
}

async fn lookup(Room { app, .. }: Room, Query(q): Query<LookupQuery>) -> AppResult<Json<serde_json::Value>> {
    let s = app.llm_settings().await;
    let card = crate::integrations::lookup::lookup(&s, q.kind.as_deref().unwrap_or("wiki"), &q.q).await.map_err(|e| AppError::bad_request(format!("{e:#}")))?;
    Ok(Json(json!({ "card": card })))
}

#[derive(serde::Deserialize, Default)]
struct AutofillRequest {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    query: Option<String>,
}

/// Fill an object's props/body from a reference source (book → OpenLibrary, movie → TMDB, else Wikipedia).
async fn autofill_node(Room { app, .. }: Room, Path(id): Path<String>, Json(req): Json<AutofillRequest>) -> AppResult<Json<serde_json::Value>> {
    let id = mermaid::safe_id(&id);
    let Some(n) = db::get_node(&app.db, &id).await? else { return Err(AppError::bad_request("нет такого объекта")) };
    let kind = req.kind.clone().or_else(|| crate::integrations::lookup::kind_for_type(&n.type_id).map(str::to_string)).unwrap_or_else(|| "wiki".into());
    let q = req.query.clone().unwrap_or_else(|| {
        let author = n.props.0.get("author").and_then(|a| a.as_str()).unwrap_or("");
        if author.is_empty() { n.label.clone() } else { format!("{} {}", n.label, author) }
    });
    let s = app.llm_settings().await;
    let card = crate::integrations::lookup::lookup(&s, &kind, &q).await.map_err(|e| AppError::bad_request(format!("{e:#}")))?;
    let Some(card) = card else { return Ok(Json(json!({ "found": false }))) };
    // only fill empty props; never overwrite what the player typed
    let mut props = serde_json::Map::new();
    for (k, v) in &card.props {
        let empty = n.props.0.get(k).map(|x| x.is_null() || x.as_str() == Some("")).unwrap_or(true);
        if empty {
            props.insert(k.clone(), v.clone());
        }
    }
    if !props.is_empty() {
        db::update_object_meta(&app.db, &id, None, Some(&serde_json::Value::Object(props.clone())), None).await?;
    }
    if !card.body.is_empty() && !n.body.contains(&card.body[..card.body.len().min(40)]) {
        let body = crate::markdown::append_section(&n.body, Some(&format!("Из {}", card.source)), &format!("{}{}", card.body, card.url.as_ref().map(|u| format!("\n\n[{}]({u})", card.source)).unwrap_or_default()));
        db::update_node_fields(&app.db, &id, None, Some(&body), None, None).await?;
    }
    emit_graph(&app).await?;
    Ok(Json(json!({ "found": true, "card": card, "filled": props.keys().collect::<Vec<_>>() })))
}

/// Append GitHub activity for a project's `repo` under "## GitHub".
async fn github_into_node(Room { app, .. }: Room, Path(id): Path<String>) -> AppResult<Json<serde_json::Value>> {
    let id = mermaid::safe_id(&id);
    let Some(n) = db::get_node(&app.db, &id).await? else { return Err(AppError::bad_request("нет такого объекта")) };
    let Some(repo) = n.props.0.get("repo").and_then(|r| r.as_str()).filter(|r| !r.trim().is_empty()) else { return Err(AppError::bad_request("у объекта нет свойства repo")) };
    let s = app.llm_settings().await;
    let text = crate::integrations::github::activity(&s, repo).await.map_err(|e| AppError::bad_request(format!("{e:#}")))?;
    let stamped = format!("_{}_\n\n{}", chrono::Local::now().format("%d.%m.%Y %H:%M"), text);
    let body = crate::markdown::append_section(&n.body, Some("GitHub"), &stamped);
    db::update_node_fields(&app.db, &id, None, Some(&mermaid::truncate_chars(&body, 60_000)), None, None).await?;
    emit_graph(&app).await?;
    Ok(Json(json!({ "activity": text })))
}

async fn weather(Room { app, .. }: Room) -> AppResult<Json<serde_json::Value>> {
    let s = app.llm_settings().await;
    let Some(city) = s.get("WEATHER_CITY") else { return Ok(Json(json!({ "configured": false }))) };
    let w = crate::integrations::weather::current(city).await.map_err(|e| AppError::bad_request(format!("{e:#}")))?;
    Ok(Json(json!({ "configured": true, "line": crate::integrations::weather::line(&w), "weather": w })))
}

async fn calendar_ics(Room { app, .. }: Room) -> AppResult<Response> {
    let ics = crate::integrations::calendar::ics(&app).await?;
    Ok(([(header::CONTENT_TYPE, "text/calendar; charset=utf-8"), (header::CONTENT_DISPOSITION, "inline; filename=\"triad-canvas.ics\"")], ics).into_response())
}

#[derive(serde::Deserialize)]
struct DaysQuery {
    days: Option<i64>,
}

async fn calendar_upcoming(Room { app, .. }: Room, Query(q): Query<DaysQuery>) -> AppResult<Json<serde_json::Value>> {
    let ev = crate::integrations::calendar::upcoming(&app, q.days.unwrap_or(7)).await?;
    Ok(Json(json!({ "events": ev, "digest": crate::integrations::calendar::digest(&ev) })))
}

#[derive(serde::Deserialize)]
struct TtsRequest {
    text: String,
    #[serde(default)]
    agent: Option<String>,
}

async fn tts(Room { app, .. }: Room, Json(req): Json<TtsRequest>) -> AppResult<Response> {
    let s = app.llm_settings().await;
    let agent_id = req.agent.clone().unwrap_or_default();
    let voice = db::list_agents(&app.db).await?.into_iter().find(|a| a.id == agent_id).map(|a| a.voice).unwrap_or_default();
    let (bytes, ct) = crate::integrations::speech::tts(&s, &req.text, &agent_id, &voice).await.map_err(|e| AppError::bad_request(format!("{e:#}")))?;
    Ok(([(header::CONTENT_TYPE, ct), (header::CACHE_CONTROL, "no-store".to_string())], bytes).into_response())
}

async fn stt(Room { app, .. }: Room, mp: Multipart) -> AppResult<Json<serde_json::Value>> {
    let (files, _) = read_multipart(mp).await?;
    let Some(f) = files.into_iter().next() else { return Err(AppError::bad_request("нет аудио")) };
    let s = app.llm_settings().await;
    let mime = f.mime.clone().unwrap_or_else(|| "audio/webm".into());
    let text = crate::integrations::speech::stt(&s, f.bytes, &f.name, &mime).await.map_err(|e| AppError::bad_request(format!("{e:#}")))?;
    Ok(Json(json!({ "text": text })))
}

async fn poll_feeds(Room { app, .. }: Room) -> AppResult<Json<serde_json::Value>> {
    let n = crate::integrations::feeds::poll(&app).await.map_err(|e| AppError::bad_request(format!("{e:#}")))?;
    db::meta_set(&app.db, "feeds_last_poll", &chrono::Utc::now().to_rfc3339()).await?;
    emit_graph(&app).await?;
    Ok(Json(json!({ "created": n })))
}

// ───────────────────────────── Messages: history, search, edit ─────────────────────────────

#[derive(serde::Deserialize)]
struct OlderQuery {
    before: Option<i64>,
    around: Option<i64>,
    limit: Option<usize>,
}

async fn older_messages(Room { app, .. }: Room, Query(q): Query<OlderQuery>) -> AppResult<Json<Vec<crate::models::Message>>> {
    let limit = q.limit.unwrap_or(60).clamp(1, 200);
    let msgs = match (q.around, q.before) {
        (Some(id), _) => db::messages_around(&app.db, id, limit / 2).await?,
        (None, Some(b)) => db::messages_before(&app.db, b, limit).await?,
        _ => db::recent_messages(&app.db, limit).await?,
    };
    Ok(Json(db::with_mentions(&app.db, msgs).await?))
}

#[derive(serde::Deserialize)]
struct SearchMsgQuery {
    q: String,
    limit: Option<usize>,
}

async fn search_messages(Room { app, .. }: Room, Query(q): Query<SearchMsgQuery>) -> AppResult<Json<Vec<crate::models::Message>>> {
    if q.q.trim().chars().count() < 2 {
        return Ok(Json(vec![]));
    }
    Ok(Json(db::search_messages(&app.db, &q.q, q.limit.unwrap_or(40).clamp(1, 200)).await?))
}

#[derive(serde::Deserialize)]
struct EditMessage {
    content: String,
}

/// Edit your own line (agents' lines are theirs). Mentions stay; a round is not re-run.
async fn edit_message(Room { app, .. }: Room, Path(id): Path<i64>, Json(req): Json<EditMessage>) -> AppResult<Json<crate::models::Message>> {
    let Some(m) = db::get_message(&app.db, id).await? else { return Err(AppError::bad_request("нет такого сообщения")) };
    if m.sender_id != USER_ID {
        return Err(AppError::bad_request("править можно только свои реплики"));
    }
    let content = req.content.trim();
    if content.is_empty() || content.chars().count() > 4000 {
        return Err(AppError::bad_request("пустое или слишком длинное"));
    }
    db::update_message_content(&app.db, id, content).await?;
    let updated = db::with_mentions(&app.db, vec![db::get_message(&app.db, id).await?.unwrap()]).await?.remove(0);
    app.emit(RoomEvent::Message { message: updated.clone() });
    Ok(Json(updated))
}

/// Delete any line from the room's history (yours or theirs — it's your room).
async fn delete_message(Room { app, .. }: Room, Path(id): Path<i64>) -> AppResult<StatusCode> {
    if !db::delete_message(&app.db, id).await? {
        return Err(AppError::bad_request("нет такого сообщения"));
    }
    app.emit(RoomEvent::MessageDeleted { id });
    Ok(StatusCode::NO_CONTENT)
}

async fn llm_log(State(app): State<AppState>) -> Json<Vec<crate::openrouter::CallLog>> {
    Json(app.llm.journal())
}

// ───────────────────────────── Organism ─────────────────────────────

async fn organism_state(Room { app, .. }: Room) -> AppResult<Json<serde_json::Value>> {
    let health = crate::organism::health(&app).await?;
    let care = crate::organism::needs_care(&app, 8).await?;
    let recent = crate::organism::recent_care(&app, 30).await?;
    Ok(Json(json!({ "health": health, "needs_care": care, "recent": recent })))
}

#[derive(serde::Deserialize, Default)]
struct TendRequest {
    #[serde(default)]
    node_id: Option<String>,
    #[serde(default)]
    agent_id: Option<String>,
}

/// Tend one object now (or whatever needs it most).
async fn organism_tend(Room { app, .. }: Room, Json(req): Json<TendRequest>) -> AppResult<Json<serde_json::Value>> {
    if app.turn_lock.try_lock().is_err() {
        return Err(AppError::bad_request("идёт раунд — чуть позже"));
    }
    let _turn = app.turn_lock.lock().await;
    let result = match req.node_id.as_deref().map(mermaid::safe_id) {
        Some(id) => {
            let agents = db::list_agents(&app.db).await?;
            let node = db::get_node(&app.db, &id).await?.ok_or_else(|| AppError::bad_request("нет такого объекта"))?;
            let atts = db::attachments_of(&app.db, &id).await?;
            let kind = if atts.iter().any(|a| a.text_len > 0 && !node.body.contains(&format!("<!-- digest:{} -->", a.id))) { "digest" } else { "manual" };
            let agent = match req.agent_id.as_deref() {
                Some(a) => agents.iter().find(|x| x.id == a && x.is_alive()).cloned(),
                None => crate::organism::pick_carer(&agents, &node, kind).cloned(),
            }
            .ok_or_else(|| AppError::bad_request("некому: все жители мертвы или уехали"))?;
            let (recs, say) = crate::organism::tend(&app, &agent.id, &id, kind).await.map_err(|e| AppError::bad_request(format!("{e:#}")))?;
            json!({ "agent": agent.id, "node": id, "actions": recs, "say": say })
        }
        None => match crate::organism::tend_next(&app).await.map_err(|e| AppError::bad_request(format!("{e:#}")))? {
            Some((agent, node, n)) => json!({ "agent": agent, "node": node, "actions_count": n }),
            None => json!({ "nothing": true, "message": "в библиотеке всё прибрано" }),
        },
    };
    app.emit(RoomEvent::Done);
    Ok(Json(result))
}

async fn organism_chronicle(Room { app, .. }: Room) -> AppResult<Json<serde_json::Value>> {
    if app.turn_lock.try_lock().is_err() {
        return Err(AppError::bad_request("идёт раунд — чуть позже"));
    }
    let _turn = app.turn_lock.lock().await;
    let id = crate::organism::chronicle(&app, true).await.map_err(|e| AppError::bad_request(format!("{e:#}")))?;
    db::meta_set(&app.db, "chronicle_date", &chrono::Local::now().format("%Y-%m-%d").to_string()).await?;
    app.emit(RoomEvent::Done);
    Ok(Json(json!({ "id": id })))
}

// ───────────────────────────── Messenger: reactions, DMs ─────────────────────────────

#[derive(serde::Deserialize)]
struct ReactRequest {
    emoji: String,
}

async fn react_message(Room { app, .. }: Room, Path(id): Path<i64>, Json(req): Json<ReactRequest>) -> AppResult<Json<crate::models::Message>> {
    let emoji: String = req.emoji.trim().chars().take(4).collect();
    if emoji.is_empty() {
        return Err(AppError::bad_request("нужен emoji"));
    }
    let Some(m) = db::toggle_reaction(&app.db, id, &emoji, USER_ID).await? else { return Err(AppError::bad_request("нет такого сообщения")) };
    let full = db::with_mentions(&app.db, vec![m]).await?.remove(0);
    app.emit(RoomEvent::Message { message: full.clone() });
    Ok(Json(full))
}

#[derive(serde::Deserialize)]
struct DmQuery {
    before: Option<i64>,
    limit: Option<usize>,
}

async fn dm_history(Room { app, .. }: Room, Path(agent): Path<String>, Query(q): Query<DmQuery>) -> AppResult<Json<Vec<crate::models::Message>>> {
    let agent = mermaid::safe_id(&agent);
    Ok(Json(db::with_mentions(&app.db, db::channel_messages(&app.db, &agent, q.limit.unwrap_or(60).clamp(1, 200), q.before).await?).await?))
}

async fn dm_overview(Room { app, .. }: Room) -> AppResult<Json<serde_json::Value>> {
    let unread = db::dm_unread(&app.db).await?;
    let agents = db::list_agents(&app.db).await?;
    let mut out = vec![];
    for a in agents {
        let last = db::channel_messages(&app.db, &a.id, 1, None).await?.pop();
        out.push(json!({ "agent": a.id, "unread": unread.iter().find(|(c, _)| c == &a.id).map(|(_, n)| *n).unwrap_or(0), "last": last }));
    }
    Ok(Json(json!({ "channels": out })))
}

/// A private line to one inhabitant: they answer alone (think → speak → tools), no Caster.
async fn dm_send(Room { app, .. }: Room, Path(agent): Path<String>, Json(req): Json<ChatRequest>) -> AppResult<Json<Vec<crate::models::Message>>> {
    let agent = mermaid::safe_id(&agent);
    let agents = db::list_agents(&app.db).await?;
    let Some(a) = agents.iter().find(|a| a.id == agent) else { return Err(AppError::bad_request("нет такого жителя")) };
    if !a.is_alive() {
        return Err(AppError::bad_request(format!("{} сейчас не отвечает", a.name)));
    }
    let text = req.message.trim();
    if text.is_empty() || text.chars().count() > 4000 {
        return Err(AppError::bad_request("пустое или слишком длинное"));
    }
    let msg = db::insert_message_in(&app.db, &agent, USER_ID, None, text, Some(&agent), None, None, req.reply_to).await?;
    app.emit(RoomEvent::Message { message: msg.clone() });
    let _turn = app.turn_lock.lock().await;
    app.emit(RoomEvent::Status { stage: "agent".into(), detail: format!("{} отвечает лично…", a.name) });
    let plan = agent_runner::TurnPlan { beat: None, scene_so_far: vec![], room_mood: None, focus_node: None, library: None, pre: None, pre_records: vec![], channel: Some(agent.clone()), reply_to: Some(msg.id) };
    let reply = agent_runner::run_agent(&app, &agent, plan).await.map_err(|e| AppError::bad_request(format!("{e:#}")))?;
    app.emit(RoomEvent::Done);
    Ok(Json(vec![msg, reply]))
}

// ───────────────────────────── Proposals ─────────────────────────────

#[derive(serde::Deserialize)]
struct ProposalsQuery {
    status: Option<String>,
}

async fn list_proposals(Room { app, .. }: Room, Query(q): Query<ProposalsQuery>) -> AppResult<Json<Vec<crate::models::Proposal>>> {
    Ok(Json(db::list_proposals(&app.db, q.status.as_deref(), 100).await?))
}

async fn accept_proposal(Room { app, .. }: Room, Path(id): Path<i64>) -> AppResult<Json<serde_json::Value>> {
    let Some(p) = db::get_proposal(&app.db, id).await? else { return Err(AppError::bad_request("нет такого предложения")) };
    if p.status != "pending" {
        return Err(AppError::bad_request("уже решено"));
    }
    let result = crate::organism::apply_proposal(&app, &p).await.map_err(|e| AppError::bad_request(format!("{e:#}")))?;
    db::set_proposal_status(&app.db, id, "accepted").await?;
    db::insert_event(&app.db, "proposal_accepted", &json!({ "id": id, "kind": p.kind, "agent": p.agent_id })).await?;
    emit_graph(&app).await?;
    app.emit(RoomEvent::Types { types: db::list_types(&app.db).await? });
    app.emit(RoomEvent::Proposals { proposals: db::list_proposals(&app.db, Some("pending"), 30).await? });
    Ok(Json(json!({ "ok": true, "result": result })))
}

async fn reject_proposal(Room { app, .. }: Room, Path(id): Path<i64>) -> AppResult<Json<serde_json::Value>> {
    let Some(p) = db::get_proposal(&app.db, id).await? else { return Err(AppError::bad_request("нет такого предложения")) };
    db::set_proposal_status(&app.db, id, "rejected").await?;
    db::insert_event(&app.db, "proposal_rejected", &json!({ "id": id, "kind": p.kind, "agent": p.agent_id })).await?;
    app.emit(RoomEvent::Proposals { proposals: db::list_proposals(&app.db, Some("pending"), 30).await? });
    Ok(Json(json!({ "ok": true })))
}

async fn organism_audit(Room { app, .. }: Room) -> AppResult<Json<serde_json::Value>> {
    if app.turn_lock.try_lock().is_err() {
        return Err(AppError::bad_request("идёт раунд — чуть позже"));
    }
    let _turn = app.turn_lock.lock().await;
    let n = crate::organism::audit(&app).await.map_err(|e| AppError::bad_request(format!("{e:#}")))?;
    app.emit(RoomEvent::Done);
    Ok(Json(json!({ "proposals": n })))
}

// ───────────────────────────── Rooms (local yards) & snapshots ─────────────────────────────

async fn rooms_list(State(app): State<AppState>) -> AppResult<Json<Vec<crate::rooms::RoomInfo>>> {
    Ok(Json(app.rooms.list_local().await?))
}

#[derive(serde::Deserialize)]
struct RoomNameRequest {
    name: String,
}

async fn rooms_create(State(app): State<AppState>, Json(req): Json<RoomNameRequest>) -> AppResult<Json<crate::rooms::RoomInfo>> {
    Ok(Json(app.rooms.create_local(&req.name).await.map_err(|e| AppError::bad_request(format!("{e:#}")))?))
}

async fn rooms_rename(State(app): State<AppState>, Path(id): Path<String>, Json(req): Json<RoomNameRequest>) -> AppResult<Json<Vec<crate::rooms::RoomInfo>>> {
    app.rooms.rename_local(&id, &req.name).await.map_err(|e| AppError::bad_request(format!("{e:#}")))?;
    Ok(Json(app.rooms.list_local().await?))
}

async fn rooms_delete(State(app): State<AppState>, Path(id): Path<String>) -> AppResult<Json<Vec<crate::rooms::RoomInfo>>> {
    app.rooms.delete_local(&id).await.map_err(|e| AppError::bad_request(format!("{e:#}")))?;
    Ok(Json(app.rooms.list_local().await?))
}

async fn snapshots_list(Room { app, .. }: Room) -> AppResult<Json<Vec<crate::backup::SnapshotInfo>>> {
    Ok(Json(crate::backup::list_snapshots(&app.room_id)?))
}

async fn snapshots_create(Room { app, .. }: Room) -> AppResult<Json<crate::backup::SnapshotInfo>> {
    Ok(Json(crate::backup::snapshot(&app, "manual").await?))
}

async fn snapshots_restore(Room { app, editable, .. }: Room, Path(name): Path<String>) -> AppResult<Json<serde_json::Value>> {
    if !editable {
        return Err(AppError::bad_request("только с машины, где запущено приложение"));
    }
    let bytes = crate::backup::read_snapshot(&app.room_id, &name).map_err(|e| AppError::bad_request(format!("{e:#}")))?;
    let manifest = crate::backup::stage_restore(&app.room_id, &bytes).map_err(|e| AppError::bad_request(format!("{e:#}")))?;
    Ok(Json(json!({ "staged": true, "manifest": manifest })))
}

async fn snapshots_delete(Room { app, .. }: Room, Path(name): Path<String>) -> AppResult<StatusCode> {
    crate::backup::delete_snapshot(&app.room_id, &name).map_err(|e| AppError::bad_request(format!("{e:#}")))?;
    Ok(StatusCode::NO_CONTENT)
}

// ───────────────────────────── Backup / restore ─────────────────────────────

async fn backup_zip(Room { app, .. }: Room) -> AppResult<Response> {
    let bytes = crate::backup::make_zip(&app).await?;
    let name = format!("triad-{}-{}.zip", mermaid::safe_id(&app.room_id), chrono::Local::now().format("%Y%m%d-%H%M"));
    Ok(([(header::CONTENT_TYPE, "application/zip".to_string()), (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\""))], bytes).into_response())
}

async fn restore_upload(Room { app, editable, .. }: Room, mp: Multipart) -> AppResult<Json<serde_json::Value>> {
    if !editable {
        return Err(AppError::bad_request("восстанавливать можно только с машины, где запущено приложение"));
    }
    let (files, _) = read_multipart(mp).await?;
    let Some(f) = files.into_iter().next() else { return Err(AppError::bad_request("нет файла")) };
    let manifest = crate::backup::stage_restore(&app.room_id, &f.bytes).map_err(|e| AppError::bad_request(format!("{e:#}")))?;
    Ok(Json(json!({ "staged": true, "manifest": manifest, "message": "Бэкап подготовлен. Перезапусти приложение — комната заменится (текущая база останется рядом как .before-restore)." })))
}

async fn restore_cancel(Room { app, .. }: Room) -> StatusCode {
    crate::backup::cancel_pending(&app.room_id);
    StatusCode::NO_CONTENT
}

// ───────────────────────────── Update from source ─────────────────────────────

async fn update_check(Room { app, .. }: Room) -> AppResult<Json<crate::selfupdate::SourceUpdate>> {
    let u = crate::selfupdate::check().await.map_err(|e| AppError::bad_request(format!("{e:#}")))?;
    *app.source_update.write().await = Some(u.clone());
    Ok(Json(u))
}

async fn update_run(Room { app, editable, .. }: Room) -> AppResult<Json<serde_json::Value>> {
    if !editable {
        return Err(AppError::bad_request("обновлять можно только с машины, где запущено приложение"));
    }
    if app.turn_lock.try_lock().is_err() {
        return Err(AppError::bad_request("идёт раунд — дождись, пока договорят"));
    }
    let log = crate::selfupdate::run().await.map_err(|e| AppError::bad_request(format!("{e:#}")))?;
    Ok(Json(json!({ "ok": true, "log": log.display().to_string(), "message": "Приложение закроется; сборка идёт в отдельном окне, потом оно запустится само." })))
}

async fn update_status() -> Json<crate::selfupdate::UpdateStatus> {
    Json(crate::selfupdate::status())
}

async fn update_ack() -> StatusCode {
    crate::selfupdate::ack();
    StatusCode::NO_CONTENT
}

// ───────────────────────────── Двор: the cast ─────────────────────────────

async fn list_roles() -> Json<Vec<crate::roles::RolePreset>> {
    Json(crate::roles::presets())
}

async fn emit_agents(app: &AppState) -> anyhow::Result<()> {
    app.emit(RoomEvent::Agents { agents: db::list_agents(&app.db).await? });
    Ok(())
}

/// Invite a character: from a role preset (with optional overrides) or fully custom.
async fn create_agent(Room { app, .. }: Room, Json(req): Json<AgentCreateRequest>) -> AppResult<Json<Vec<crate::models::Agent>>> {
    let preset = req.role.as_deref().and_then(crate::roles::find);
    let name = req.name.as_deref().map(str::trim).filter(|n| n.chars().count() >= 2).map(str::to_string).or_else(|| preset.as_ref().map(|p| p.name.to_string()));
    let Some(name) = name else { return Err(AppError::bad_request("нужно имя (или роль из пресетов)")) };
    let core = req.system_core.as_deref().map(str::trim).filter(|c| c.chars().count() >= 40).map(str::to_string).or_else(|| preset.as_ref().map(|p| p.core.to_string()));
    let Some(core) = core else { return Err(AppError::bad_request("нужно описание характера (system_core, от 40 символов) или роль")) };
    let archetype = req.archetype.as_deref().map(str::trim).filter(|a| !a.is_empty()).map(str::to_string).or_else(|| preset.as_ref().map(|p| p.archetype.to_string())).unwrap_or_else(|| "Житель двора".into());
    let color = req.color.as_deref().map(str::trim).filter(|c| c.starts_with('#') && (c.len() == 7 || c.len() == 4)).map(str::to_string).or_else(|| preset.as_ref().map(|p| p.color.to_string())).unwrap_or_else(|| "#9a9ca8".into());
    let glyph = req.glyph.as_deref().map(|g| g.trim().chars().take(2).collect::<String>()).filter(|g| !g.is_empty()).or_else(|| preset.as_ref().map(|p| p.glyph.to_string())).unwrap_or_else(|| name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default());
    let agenda = req.agenda.as_deref().map(str::trim).filter(|a| !a.is_empty()).map(str::to_string).or_else(|| preset.as_ref().map(|p| p.agenda.to_string())).unwrap_or_else(|| "Присматривается к игроку и соседям.".into());
    let model = req.model.unwrap_or_default().trim().to_string();
    let voice = req.voice.unwrap_or_default().trim().to_string();
    let role = preset.as_ref().map(|p| p.id.to_string()).unwrap_or_else(|| "custom".into());
    // id: the preset id if free, else a slug of the name, else numbered
    let existing = db::list_agents(&app.db).await?;
    let mut id = preset.as_ref().map(|p| p.id.to_string()).filter(|pid| !existing.iter().any(|a| &a.id == pid)).unwrap_or_else(|| mermaid::safe_id(&name));
    if id.is_empty() || id == "user" || id == "system" {
        id = "zhitel".into();
    }
    if existing.iter().any(|a| a.id == id) {
        id = format!("{id}_{}", existing.len() + 1);
    }
    if existing.len() >= 8 {
        return Err(AppError::bad_request("во дворе уже восемь жителей — отправь кого-нибудь в отпуск"));
    }
    let (dop, resp) = preset.as_ref().map(|p| (p.dopamine, p.respect)).unwrap_or((50, 0));
    db::insert_agent(&app.db, &id, &name, &archetype, &core, &color, &glyph, &role, &model, &voice, &agenda, dop, resp).await?;
    if let Some(p) = &preset {
        let pid = format!("{}_{}", p.project.0, id);
        if !db::node_exists(&app.db, &pid).await? && !db::node_exists(&app.db, p.project.0).await? {
            db::upsert_object(&app.db, p.project.0, p.project.1, "project", &id, Some(p.project.2), Some(&json!({ "status": "в работе" })), None, None).await?;
        } else if !db::node_exists(&app.db, &pid).await? {
            db::upsert_object(&app.db, &pid, p.project.1, "project", &id, Some(p.project.2), Some(&json!({ "status": "в работе" })), None, None).await?;
        }
        db::place_unpositioned_nodes(&app.db).await?;
    }
    let msg = db::insert_message(&app.db, "system", None, &format!("Во двор заходит {name} — {}.", archetype.split('.').next().unwrap_or(&archetype).trim().to_lowercase())).await?;
    app.emit(RoomEvent::Message { message: msg });
    db::insert_event(&app.db, "agent_joined", &json!({ "id": id, "name": name })).await?;
    emit_agents(&app).await?;
    emit_graph(&app).await?;
    Ok(Json(db::list_agents(&app.db).await?))
}

async fn patch_agent(Room { app, .. }: Room, Path(id): Path<String>, Json(req): Json<AgentPatchRequest>) -> AppResult<Json<Vec<crate::models::Agent>>> {
    let id = mermaid::safe_id(&id);
    let agents = db::list_agents(&app.db).await?;
    let Some(cur) = agents.iter().find(|a| a.id == id) else { return Err(AppError::bad_request("нет такого жителя")) };
    let name = req.name.as_deref().map(str::trim).filter(|n| n.chars().count() >= 2);
    let core = req.system_core.as_deref().map(str::trim).filter(|c| c.chars().count() >= 40);
    let color = req.color.as_deref().map(str::trim).filter(|c| c.starts_with('#') && (c.len() == 7 || c.len() == 4));
    let glyph = req.glyph.as_deref().map(|g| g.trim().chars().take(2).collect::<String>()).filter(|g| !g.is_empty());
    let archetype = req.archetype.as_deref().map(str::trim).filter(|a| !a.is_empty());
    let agenda = req.agenda.as_deref().map(str::trim).filter(|a| !a.is_empty());
    let model = req.model.as_deref().map(str::trim);
    let voice = req.voice.as_deref().map(str::trim);
    if req.active == Some(false) {
        let alive_others = agents.iter().filter(|a| a.is_alive() && a.id != id).count();
        if alive_others == 0 {
            return Err(AppError::bad_request("нельзя оставить двор пустым"));
        }
    }
    db::update_agent_profile(&app.db, &id, name, archetype, core, color, glyph.as_deref(), model, voice, agenda, req.active).await?;
    if req.revive {
        db::revive_agent(&app.db, &id).await?;
    }
    if let Some(active) = req.active {
        if active != (cur.active != 0) {
            let text = if active { format!("{} возвращается во двор.", name.unwrap_or(&cur.name)) } else { format!("{} уезжает из двора. Может, вернётся.", cur.name) };
            let msg = db::insert_message(&app.db, "system", None, &text).await?;
            app.emit(RoomEvent::Message { message: msg });
            db::insert_event(&app.db, if active { "agent_returned" } else { "agent_left" }, &json!({ "id": id })).await?;
        }
    }
    emit_agents(&app).await?;
    Ok(Json(db::list_agents(&app.db).await?))
}

/// "Delete" = move out: lines and memories stay, the character stops speaking.
async fn retire_agent(room: Room, Path(id): Path<String>) -> AppResult<Json<Vec<crate::models::Agent>>> {
    patch_agent(room, Path(id), Json(AgentPatchRequest { active: Some(false), ..Default::default() })).await
}

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
async fn run_round_from(app: &AppState, trigger: Trigger, alive_before: Vec<String>, attachments: Option<String>) -> anyhow::Result<()> {
    run_round_inner(app, trigger, alive_before, attachments, 0).await
}

async fn run_round_inner(app: &AppState, trigger: Trigger, alive_before: Vec<String>, attachments: Option<String>, depth: u8) -> anyhow::Result<()> {
    // Deadlines bite before anyone speaks, so the Director sees the failure as an event.
    contracts::expire_overdue(app).await?;

    app.emit(RoomEvent::Status {
        stage: "director".into(),
        detail: "Режиссёр читает комнату и раздаёт скрытые цели…".into(),
    });
    let pipeline = app.cfg.director_mode != "single";
    let mut plan = if pipeline { Some(crate::pipeline::before_speech(app, &trigger).await?) } else { None };
    if let (Some(p), Some(a)) = (plan.as_mut(), attachments.as_ref()) {
        p.library = Some(match p.library.take() {
            Some(l) => format!("{a}\n{l}"),
            None => a.clone(),
        });
    }
    let entry = match &plan {
        Some(p) => {
            let e = db::insert_director_log(&app.db, trigger.as_str(), "pipeline", &p.output).await?;
            app.emit(RoomEvent::Director { entry: e.clone() });
            e
        }
        None => director::run_director(app, &trigger).await?,
    };
    let mood = entry.output.room_mood.clone();
    let agents = db::list_agents(&app.db).await?;
    let focus_node = match &trigger {
        Trigger::Node { id, .. } => Some(id.clone()),
        _ => None,
    };
    let mut scene_so_far: Vec<(String, String)> = Vec::new();
    let library_text: Option<String> = plan.as_ref().and_then(|p| p.library.clone());
    let mut pre_records: Vec<crate::models::ActionRecord> = plan.as_mut().map(|p| std::mem::take(&mut p.pre_records)).unwrap_or_default();
    // Everyone thinks at once; then they speak in order, each hearing the previous lines.
    let thoughts: Vec<Option<agent_runner::Thought>> = if entry.output.scene.len() > 1 && app.cfg.agent_mode != "single" {
        for b in &entry.output.scene {
            app.emit(RoomEvent::Status { stage: "agent".into(), detail: format!("{} думает…", prompts::sender_name(&b.speaker, &agents)) });
        }
        futures::future::join_all(entry.output.scene.iter().map(|b| agent_runner::pre_think(app, &b.speaker, b, mood.as_deref(), focus_node.as_deref(), library_text.as_deref()))).await
    } else {
        entry.output.scene.iter().map(|_| None).collect()
    };
    for (i, beat) in entry.output.scene.iter().enumerate() {
        let name = prompts::sender_name(&beat.speaker, &agents);
        app.emit(RoomEvent::Status {
            stage: "agent".into(),
            detail: format!("{name} формулирует…"),
        });
        let plan_turn = agent_runner::TurnPlan {
            beat: Some(beat),
            scene_so_far: scene_so_far.clone(),
            room_mood: mood.as_deref(),
            focus_node: focus_node.as_deref(),
            library: library_text.as_deref(),
            pre: thoughts.get(i).cloned().flatten(),
            pre_records: std::mem::take(&mut pre_records),
            channel: None,
            reply_to: None,
        };
        match agent_runner::run_agent(app, &beat.speaker, plan_turn).await {
            Ok(m) => scene_so_far.push((name, m.content)),
            Err(e) => tracing::warn!(agent = %beat.speaker, error = %e, "agent turn skipped"),
        }
    }
    // Big yard: the bystanders chime in (one cheap call, several short lines).
    let alive_now: Vec<_> = agents.iter().filter(|a| a.is_alive()).collect();
    if alive_now.len() > 3 && !scene_so_far.is_empty() && matches!(trigger, Trigger::UserMessage | Trigger::Node { .. } | Trigger::Tick) && app.llm.budget_left() {
        let main: Vec<String> = entry.output.scene.iter().map(|b| b.speaker.clone()).collect();
        let last_user = db::recent_messages(&app.db, 30).await?.iter().rev().find(|m| m.sender_id == USER_ID).map(|m| m.content.clone()).unwrap_or_default();
        let recent_sp = director::recent_speakers(&db::recent_messages(&app.db, 40).await?, &agents);
        let st = director::stakes(&agents, &last_user, &db::recent_messages(&app.db, 12).await?, &recent_sp, &[]);
        app.emit(RoomEvent::Status { stage: "agent".into(), detail: "остальные вставляют по слову…".into() });
        match director::chorus(app, &agents, &scene_so_far, &last_user, &main, &st).await {
            Ok(out) => {
                let recent = db::recent_messages(&app.db, 20).await?;
                for (sp, emoji, on) in out.reactions {
                    let target = recent.iter().rev().find(|m| m.sender_id == on);
                    if let Some(t) = target {
                        if let Some(m) = db::toggle_reaction(&app.db, t.id, &emoji, &sp).await? {
                            app.emit(RoomEvent::Message { message: db::with_mentions(&app.db, vec![m]).await?.remove(0) });
                        }
                    }
                }
                for (sp, to, text) in out.lines {
                    tokio::time::sleep(std::time::Duration::from_millis(450)).await;
                    let reply_to = if to != "user" && to != "room" { recent.iter().rev().find(|m| m.sender_id == to).map(|m| m.id) } else { None };
                    let msg = db::insert_message_in(&app.db, "", &sp, None, &text, Some(&to), Some("вставка"), focus_node.as_deref(), reply_to).await?;
                    app.emit(RoomEvent::Message { message: msg.clone() });
                    scene_so_far.push((prompts::sender_name(&sp, &agents), text));
                }
            }
            Err(e) => tracing::debug!(error = %e, "chorus skipped"),
        }
    }
    if let Some(p) = plan.as_mut() {
        app.emit(RoomEvent::Status { stage: "director".into(), detail: "Картограф и Психолог разбирают раунд…".into() });
        if let Err(e) = crate::pipeline::after_speech(app, &trigger, p).await {
            tracing::warn!(error = %e, "post-round pipeline failed");
        }
        let e = db::insert_director_log(&app.db, trigger.as_str(), "pipeline", &p.output).await?;
        app.emit(RoomEvent::Director { entry: e });
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
    // Lines the player dropped while we were talking get their own round (bounded).
    if app.pending.swap(0, std::sync::atomic::Ordering::Relaxed) > 0 && depth < 2 {
        let alive = alive_ids(app).await?;
        return Box::pin(run_round_inner(app, Trigger::UserMessage, alive, None, depth + 1)).await;
    }
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
        run_round_from(&app, Trigger::Event, alive_before, None).await?;
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
    app.emit(RoomEvent::Types { types: state.types.clone() });
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
