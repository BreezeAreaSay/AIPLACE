//! Triad Canvas — one binary: embedded UI + API. Runs on a laptop today and on a VPS behind
//! a Telegram Mini App tomorrow; the frontend never talks to anything but `/api/*`.

mod agent_runner;
mod cli;
mod config;
mod contracts;
mod db;
mod director;
mod error;
mod handlers;
mod memory;
mod mermaid;
mod mock_llm;
mod models;
mod openrouter;
mod prompts;
mod state;
mod update;
mod web;

use std::{net::SocketAddr, path::Path, sync::Arc};

use anyhow::Context;
use clap::Parser;
use tokio::sync::{broadcast, Mutex, RwLock};
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing_subscriber::EnvFilter;

use crate::{
    cli::{Cli, Cmd},
    config::{Config, LlmSettings},
    openrouter::LlmClient,
    state::AppState,
};

fn main() {
    if let Err(e) = real_main() {
        eprintln!("✗ {e:#}");
        std::process::exit(1);
    }
}

fn real_main() -> anyhow::Result<()> {
    config::load_env_files();
    let cli = Cli::parse();
    if let Some(p) = cli.port {
        std::env::set_var("PORT", p.to_string());
    }
    if let Some(h) = &cli.host {
        std::env::set_var("HOST", h);
    }

    match cli.cmd.unwrap_or(Cmd::Serve) {
        Cmd::Setup => {
            cli::setup_wizard()?;
            if cli::interactive() {
                print!("Запустить комнату сейчас? [Y/n] ");
                use std::io::Write;
                std::io::stdout().flush()?;
                let mut a = String::new();
                std::io::stdin().read_line(&mut a)?;
                if !matches!(a.trim().to_lowercase().as_str(), "n" | "no" | "н" | "нет") {
                    return serve(true);
                }
            }
            Ok(())
        }
        Cmd::Key { value } => cli::set_key(value),
        Cmd::Config => {
            cli::show_config();
            Ok(())
        }
        Cmd::Update { check } => update::self_update(check),
        Cmd::Serve => serve(cli.no_prompt),
    }
}

fn serve(no_prompt: bool) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,sqlx=warn,tower_http=warn".into()),
        )
        .init();

    let mut settings = LlmSettings::from_env();
    if settings.mock_mode() && !no_prompt && cli::interactive() {
        if let Some(s) = cli::first_run_prompt()? {
            settings = s;
        }
    }
    let cfg = Config::from_env()?;

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("tokio runtime")?
        .block_on(run(cfg, settings))
}

async fn run(cfg: Config, settings: LlmSettings) -> anyhow::Result<()> {
    std::fs::create_dir_all(config::triad_home()).ok();
    let pool = db::connect(&cfg.database_url).await?;
    db::migrate(&pool).await?;
    db::seed_if_empty(&pool).await?;

    let settings = Arc::new(RwLock::new(settings));
    let (events, _) = broadcast::channel(256);
    let state = AppState {
        cfg: Arc::new(cfg.clone()),
        settings: settings.clone(),
        db: pool,
        llm: Arc::new(LlmClient::new(settings.clone(), cfg.app_url.clone())?),
        events,
        turn_lock: Arc::new(Mutex::new(())),
        update: Arc::new(RwLock::new(None)),
    };

    if cfg.update_check {
        let slot = state.update.clone();
        tokio::spawn(async move {
            if let Some(info) = update::check_latest().await {
                tracing::info!("доступна новая версия v{} (у тебя v{}) → `triad update`", info.latest, info.current);
                *slot.write().await = Some(info);
            }
        });
    }

    let mut app = handlers::router(state)
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

    let addr = format!("{}:{}", cfg.host, cfg.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("cannot bind {addr}"))?;
    banner(&cfg, &*settings.read().await).await;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .context("server error")?;
    Ok(())
}

async fn banner(cfg: &Config, s: &LlmSettings) {
    let url = format!("http://localhost:{}", cfg.port);
    println!("┌─ TRIAD CANVAS v{} ─────────────────────────────────", update::VERSION);
    println!("│ UI / API : {url}   (bind {}:{})", cfg.host, cfg.port);
    println!("│ конфиг   : {}", config::config_path().display());
    match s.key_hint() {
        Some(h) => println!("│ LLM      : {} · режиссёр {} · ключ {h}", s.model, s.director_model),
        None => println!("│ LLM      : MOCK-режим (нет ключа) → `triad setup`, `triad key sk-or-…` или ⚙ в UI"),
    }
    if !web::is_embedded() {
        println!("│ UI       : не встроен в бинарник → `cd frontend && npm run dev`");
    }
    println!("└──────────────────────────────────────────────────────────");
}
