//! `triad` command line: serve (default), setup wizard, key, config, update.

use std::io::{self, BufRead, IsTerminal, Write};

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::config::{self, LlmSettings, DEFAULT_MODEL};

#[derive(Parser, Debug)]
#[command(
    name = "triad",
    version,
    about = "Triad Canvas — киберпсихологическая комната трёх ИИ-субъектов",
    long_about = None,
    arg_required_else_help = false
)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Option<Cmd>,
    /// Порт HTTP-сервера (иначе PORT из конфига, по умолчанию 3000)
    #[arg(long, global = true)]
    pub port: Option<u16>,
    /// Адрес для bind (иначе HOST, по умолчанию 0.0.0.0)
    #[arg(long, global = true)]
    pub host: Option<String>,
    /// Не спрашивать ключ в терминале при первом запуске
    #[arg(long, global = true)]
    pub no_prompt: bool,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Запустить сервер + UI (по умолчанию)
    Serve,
    /// Мастер настройки: ключ OpenRouter, модели, порт → ~/.triad/config.env
    Setup,
    /// Записать ключ OpenRouter без мастера (пустое значение — удалить ключ)
    Key {
        /// sk-or-v1-…
        value: Option<String>,
    },
    /// Показать, где лежит конфиг и что в нём (ключ маскируется)
    Config,
    /// Обновиться до последнего релиза с GitHub
    Update {
        /// Только проверить, ничего не ставить
        #[arg(long)]
        check: bool,
    },
}

fn ask(prompt: &str, default: &str) -> Result<String> {
    let mut out = io::stdout().lock();
    if default.is_empty() {
        write!(out, "{prompt}: ")?;
    } else {
        write!(out, "{prompt} [{default}]: ")?;
    }
    out.flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let v = line.trim().to_string();
    Ok(if v.is_empty() { default.to_string() } else { v })
}

fn mask(t: &str) -> String {
    let n = t.chars().count();
    if n <= 10 {
        return "•".repeat(n);
    }
    format!("{}…{}", t.chars().take(6).collect::<String>(), t.chars().skip(n - 4).collect::<String>())
}

pub fn interactive() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

/// Full wizard. Returns the saved settings.
pub fn setup_wizard() -> Result<LlmSettings> {
    let current = LlmSettings::from_env();
    println!("┌─ TRIAD CANVAS · настройка ──────────────────────────────────");
    println!("│ Конфиг: {}", config::config_path().display());
    println!("│ Ключ OpenRouter: https://openrouter.ai/keys  (пусто = mock-режим без LLM)");
    let key_default = current.key_hint().unwrap_or_default();
    let key_in = ask("│ OPENROUTER_API_KEY", &key_default)?;
    let api_key = if key_in == key_default {
        current.api_key.clone()
    } else if key_in.is_empty() || key_in == "-" {
        None
    } else {
        Some(key_in)
    };
    println!("│ Модель агентов: нужна поддержка assistant-prefill (deepseek / anthropic / google / meta).");
    let model = ask("│ OPENROUTER_MODEL", &current.model)?;
    println!("│ Режиссёр: быстрая JSON-модель, напр. google/gemini-2.5-flash или та же.");
    let director_model = ask("│ OPENROUTER_DIRECTOR_MODEL", &current.director_model)?;
    let port = ask("│ PORT", &std::env::var("PORT").unwrap_or_else(|_| "3000".into()))?;
    println!("│ Telegram Mini App (необязательно): токен бота от @BotFather. Пусто — локальный режим.");
    let tg_default = std::env::var("TELEGRAM_BOT_TOKEN").ok().map(|t| mask(&t)).unwrap_or_default();
    let tg_in = ask("│ TELEGRAM_BOT_TOKEN", &tg_default)?;
    let tg_token = if tg_in == tg_default { std::env::var("TELEGRAM_BOT_TOKEN").unwrap_or_default() } else if tg_in == "-" { String::new() } else { tg_in };
    let mut extra: Vec<(&str, String)> = vec![("PORT", port.clone()), ("TELEGRAM_BOT_TOKEN", tg_token.clone())];
    if !tg_token.is_empty() {
        println!("│ Публичный https-адрес, по которому Telegram откроет мини-апп (туннель или домен VPS).");
        let url = ask("│ WEBAPP_URL", &std::env::var("WEBAPP_URL").unwrap_or_default())?;
        println!("│ Твой Telegram user id (число, узнать: @userinfobot) — сможешь менять ключ из мини-аппа.");
        let admin = ask("│ TELEGRAM_ADMIN_IDS", &std::env::var("TELEGRAM_ADMIN_IDS").unwrap_or_default())?;
        extra.push(("WEBAPP_URL", url.clone()));
        extra.push(("APP_URL", url));
        extra.push(("TELEGRAM_ADMIN_IDS", admin));
    }
    let settings = LlmSettings { api_key, model, director_model, integrations: LlmSettings::from_env().integrations };
    let path = settings.save()?;
    config::save_config_values(&extra)?;
    for (k, v) in &extra {
        if v.is_empty() { std::env::remove_var(k) } else { std::env::set_var(k, v) }
    }
    println!("│ ✔ Сохранено в {}", path.display());
    println!("└─────────────────────────────────────────────────────────────");
    Ok(settings)
}

/// Lightweight first-run prompt when serving without a key from a terminal.
pub fn first_run_prompt() -> Result<Option<LlmSettings>> {
    println!("Ключ OpenRouter не найден. Вставь его сейчас (Enter — пропустить и запустить mock-режим):");
    let key = ask("OPENROUTER_API_KEY", "")?;
    if key.is_empty() {
        return Ok(None);
    }
    let mut s = LlmSettings::from_env();
    s.api_key = Some(key);
    if s.model.is_empty() {
        s.model = DEFAULT_MODEL.into();
    }
    let path = s.save()?;
    println!("✔ Ключ сохранён в {}", path.display());
    Ok(Some(s))
}

pub fn set_key(value: Option<String>) -> Result<()> {
    let mut s = LlmSettings::from_env();
    s.api_key = value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let path = s.save()?;
    match s.key_hint() {
        Some(h) => println!("✔ Ключ {h} сохранён в {}", path.display()),
        None => println!("✔ Ключ удалён из {} (mock-режим)", path.display()),
    }
    Ok(())
}

pub fn show_config() {
    let path = config::config_path();
    println!("TRIAD_HOME : {}", config::triad_home().display());
    println!("config     : {} {}", path.display(), if path.exists() { "" } else { "(ещё не создан — triad setup)" });
    let s = LlmSettings::from_env();
    println!("api key    : {}", s.key_hint().unwrap_or_else(|| "— (mock-режим)".into()));
    println!("model      : {}", s.model);
    println!("director   : {}", s.director_model);
    for (k, v) in config::read_config_file() {
        if k == "OPENROUTER_API_KEY" {
            continue;
        }
        if k == "TELEGRAM_BOT_TOKEN" {
            println!("{k:<11}: {}", mask(&v));
            continue;
        }
        println!("{k:<11}: {v}");
    }
    println!("version    : v{}", crate::update::VERSION);
}
