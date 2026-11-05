//! Triad Canvas backend as a library: the `triad` CLI and the Tauri desktop app both embed
//! this server. Everything the UI needs is behind `/api/*` on a loopback (or public) port.

pub mod agent_runner;
pub mod cli;
pub mod config;
pub mod contracts;
pub mod db;
pub mod director;
pub mod error;
pub mod handlers;
pub mod memory;
pub mod mermaid;
pub mod mock_llm;
pub mod models;
pub mod openrouter;
pub mod prompts;
pub mod rooms;
pub mod state;
pub mod telegram;
pub mod update;
pub mod web;

use std::{net::SocketAddr, path::Path, sync::Arc};

use anyhow::Context;
use tokio::{net::TcpListener, sync::RwLock};
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

pub use config::{Config, LlmSettings};
pub use state::AppState;

/// Bind `host:port`; if `port` is busy (or 0) fall back to an OS-assigned free port.
pub async fn bind(host: &str, port: u16) -> anyhow::Result<TcpListener> {
    match TcpListener::bind((host, port)).await {
        Ok(l) => Ok(l),
        Err(e) if port != 0 => {
            tracing::warn!(port, error = %e, "port busy → picking a free one");
            TcpListener::bind((host, 0)).await.with_context(|| format!("cannot bind {host}"))
        }
        Err(e) => Err(e).with_context(|| format!("cannot bind {host}:{port}")),
    }
}

/// Build the router and shared state (opens the local room, starts the Telegram bot and the
/// update check when configured).
pub async fn build_app(cfg: Config, settings: LlmSettings) -> anyhow::Result<(axum::Router, AppState)> {
    std::fs::create_dir_all(config::triad_home()).ok();
    let cfg = Arc::new(cfg);
    let rooms = Arc::new(rooms::RoomManager::new(cfg.clone()));
    let local = rooms.get(rooms::LOCAL_ROOM).await?;

    let settings = Arc::new(RwLock::new(settings));
    let state = AppState {
        cfg: cfg.clone(),
        settings: settings.clone(),
        llm: Arc::new(openrouter::LlmClient::new(settings.clone(), cfg.app_url.clone())?),
        update: Arc::new(RwLock::new(None)),
        rooms,
        bot_username: Arc::new(RwLock::new(None)),
        room_id: rooms::LOCAL_ROOM.to_string(),
        db: local.db,
        events: local.events,
        turn_lock: local.turn_lock,
    };

    if cfg.telegram_bot_token.is_some() && cfg.telegram_polling {
        tokio::spawn(telegram::run_bot(cfg.clone(), state.bot_username.clone()));
    }
    if cfg.update_check {
        let slot = state.update.clone();
        tokio::spawn(async move {
            if let Some(info) = update::check_latest().await {
                tracing::info!("доступна новая версия v{} (у тебя v{}) → `triad update`", info.latest, info.current);
                *slot.write().await = Some(info);
            }
        });
    }

    let mut app = handlers::router(state.clone())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    // UI: explicit on-disk folder (dev / custom) → embedded bundle (release) → hint page.
    let on_disk = cfg.static_dir.as_deref().map(Path::new).filter(|p| p.join("index.html").exists());
    if let Some(dir) = on_disk {
        tracing::info!(dir = %dir.display(), "serving UI from disk");
        let index = ServeFile::new(dir.join("index.html"));
        app = app.fallback_service(ServeDir::new(dir).not_found_service(index));
    } else {
        app = app.fallback(web::serve);
    }
    Ok((app, state))
}

/// Serve forever on an already-bound listener.
pub async fn serve(listener: TcpListener, cfg: Config, settings: LlmSettings) -> anyhow::Result<()> {
    let (app, _state) = build_app(cfg, settings).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .context("server error")
}
