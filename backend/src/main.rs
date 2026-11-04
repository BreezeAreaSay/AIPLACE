//! Triad Canvas backend — one binary that serves the API today on localhost and tomorrow
//! on a VPS behind a Telegram Mini App. The frontend never talks to anything but `/api/*`.

mod agent_runner;
mod config;
mod db;
mod director;
mod error;
mod handlers;
mod mermaid;
mod mock_llm;
mod models;
mod openrouter;
mod prompts;
mod state;

use std::{path::Path, sync::Arc};

use anyhow::Context;
use tokio::sync::{broadcast, Mutex};
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing_subscriber::EnvFilter;

use crate::{config::Config, openrouter::LlmClient, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,sqlx=warn,tower_http=info".into()),
        )
        .init();

    let cfg = Config::from_env()?;
    if cfg.mock_mode() {
        tracing::warn!("OPENROUTER_API_KEY not set → MOCK MODE (template replies). Put a key into backend/.env for the real Triad.");
    } else {
        tracing::info!(model = %cfg.openrouter_model, director = %cfg.director_model, "OpenRouter configured");
    }

    let pool = db::connect(&cfg.database_url).await?;
    db::migrate(&pool).await?;
    db::seed_if_empty(&pool).await?;

    let (events, _) = broadcast::channel(256);
    let state = AppState {
        cfg: Arc::new(cfg.clone()),
        db: pool,
        llm: Arc::new(LlmClient::new(cfg.clone())?),
        events,
        turn_lock: Arc::new(Mutex::new(())),
    };

    let mut app = handlers::router(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    // Same binary can serve the built SPA (frontend/dist) — single process on a $5 VPS.
    let static_dir = Path::new(&cfg.static_dir);
    if static_dir.join("index.html").exists() {
        tracing::info!(dir = %static_dir.display(), "serving frontend build");
        let index = ServeFile::new(static_dir.join("index.html"));
        app = app.fallback_service(ServeDir::new(static_dir).not_found_service(index));
    } else {
        tracing::info!("no frontend build found at {} → API only (run `npm run dev` in frontend/)", static_dir.display());
    }

    let addr = format!("{}:{}", cfg.host, cfg.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("cannot bind {addr}"))?;
    tracing::info!("Triad Canvas backend listening on http://{addr}");
    axum::serve(listener, app).await.context("server error")?;
    Ok(())
}
