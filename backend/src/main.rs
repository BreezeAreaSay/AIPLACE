//! `triad` CLI: setup / key / config / update, and `serve` (default) — the same server the
//! desktop app embeds.

use std::io::Write;

use anyhow::Context;
use clap::Parser;
use triad_canvas_backend::{
    cli::{self, Cli, Cmd},
    config::{self, Config, LlmSettings},
    update, web,
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
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn,tower_http=warn".into()),
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
        .block_on(async {
            let listener = tokio::net::TcpListener::bind((cfg.host.as_str(), cfg.port))
                .await
                .with_context(|| format!("cannot bind {}:{}", cfg.host, cfg.port))?;
            banner(&cfg, &settings);
            triad_canvas_backend::serve(listener, cfg, settings).await
        })
}

fn banner(cfg: &Config, s: &LlmSettings) {
    let url = format!("http://localhost:{}", cfg.port);
    println!("┌─ TRIAD CANVAS v{} ─────────────────────────────────", update::VERSION);
    println!("│ UI / API : {url}   (bind {}:{})", cfg.host, cfg.port);
    println!("│ конфиг   : {}", config::config_path().display());
    match s.key_hint() {
        Some(h) => println!("│ LLM      : {} · режиссёр {} · ключ {h}", s.model, s.director_model),
        None => println!("│ LLM      : MOCK-режим (нет ключа) → `triad setup`, `triad key sk-or-…` или ⚙ в UI"),
    }
    if cfg.telegram_bot_token.is_some() {
        println!(
            "│ Telegram : ПУБЛИЧНЫЙ режим · комната на каждого пользователя · Mini App URL {}{}",
            cfg.webapp_url,
            if cfg.allow_anon_rooms { " · анонимы разрешены" } else { " · без Telegram — только localhost" }
        );
    } else {
        println!("│ Telegram : выкл (TELEGRAM_BOT_TOKEN не задан) → одна локальная комната");
    }
    if !web::is_embedded() {
        println!("│ UI       : не встроен в бинарник → `cd frontend && npm run dev`");
    }
    println!("└──────────────────────────────────────────────────────────");
}
