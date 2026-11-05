//! Runtime configuration.
//!
//! Precedence (highest first): process env → `./.env` (dev) → `$TRIAD_HOME/config.env`
//! (written by `triad setup` / the UI) → defaults. `TRIAD_HOME` defaults to `~/.triad` and
//! also holds the SQLite database, so an installed release never touches the working dir.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

pub const DEFAULT_MODEL: &str = "deepseek/deepseek-chat";
pub const CONFIG_FILE: &str = "config.env";

pub fn triad_home() -> PathBuf {
    if let Some(h) = std::env::var_os("TRIAD_HOME") {
        return PathBuf::from(h);
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".triad")
}

pub fn config_path() -> PathBuf {
    triad_home().join(CONFIG_FILE)
}

/// Load `.env` (cwd, dev convenience) and then the persisted user config. Neither overrides
/// variables that are already set, which is exactly the precedence we want.
pub fn load_env_files() {
    dotenvy::dotenv().ok();
    let p = config_path();
    if p.exists() {
        dotenvy::from_path(&p).ok();
    }
}

fn env_or(key: &str, default: &str) -> String {
    env_opt(key).unwrap_or_else(|| default.to_string())
}

fn env_opt(key: &str) -> Option<String> {
    std::env::var(key).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn env_bool(key: &str) -> bool {
    matches!(env_opt(key).as_deref(), Some("1" | "true" | "yes" | "on"))
}

/// Static, process-lifetime settings.
#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub host: String,
    pub port: u16,
    /// Serve the UI from disk instead of the embedded bundle (dev / custom builds).
    pub static_dir: Option<String>,
    pub app_url: String,
    pub context_messages: usize,
    pub tick_entropy_hp: i64,
    pub tick_entropy_dopamine: i64,
    /// Allow `POST /api/settings` from non-loopback clients (off by default: on a public VPS
    /// anyone opening the Mini App could otherwise swap your API key).
    pub allow_remote_settings: bool,
    pub update_check: bool,
    /// Rewrite each agent's diary after this many new messages.
    pub memory_every: usize,
    /// Telegram Mini App mode: validate initData, one room per Telegram user, /start bot.
    pub telegram_bot_token: Option<String>,
    pub telegram_admin_ids: Vec<i64>,
    pub telegram_polling: bool,
    /// Let browsers without Telegram initData use the shared `local` room even in public mode.
    pub allow_anon_rooms: bool,
    /// Public https URL of the Mini App (sent in the /start button). Defaults to APP_URL.
    pub webapp_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let port: u16 = env_or("PORT", "3000").parse().context("PORT must be a number")?;
        let home = triad_home();
        let default_db = format!("sqlite://{}?mode=rwc", home.join("triad.db").display());
        Ok(Self {
            database_url: env_or("DATABASE_URL", &default_db),
            host: env_or("HOST", "0.0.0.0"),
            port,
            static_dir: env_opt("STATIC_DIR"),
            app_url: env_or("APP_URL", &format!("http://localhost:{port}")),
            context_messages: env_or("CONTEXT_MESSAGES", "14").parse().unwrap_or(14),
            tick_entropy_hp: env_or("TICK_ENTROPY_HP", "2").parse().unwrap_or(2),
            tick_entropy_dopamine: env_or("TICK_ENTROPY_DOPAMINE", "3").parse().unwrap_or(3),
            allow_remote_settings: env_bool("ALLOW_REMOTE_SETTINGS"),
            update_check: !env_bool("TRIAD_NO_UPDATE_CHECK"),
            memory_every: env_or("MEMORY_EVERY", "8").parse().unwrap_or(8),
            telegram_bot_token: env_opt("TELEGRAM_BOT_TOKEN"),
            telegram_admin_ids: env_or("TELEGRAM_ADMIN_IDS", "")
                .split([',', ' ', ';'])
                .filter_map(|x| x.trim().parse().ok())
                .collect(),
            telegram_polling: env_opt("TELEGRAM_POLLING").map(|v| matches!(v.as_str(), "1" | "true" | "yes" | "on")).unwrap_or(true),
            allow_anon_rooms: env_bool("ALLOW_ANON_ROOMS"),
            webapp_url: env_or("WEBAPP_URL", &env_or("APP_URL", &format!("http://localhost:{port}"))),
        })
    }
}

/// Hot-swappable LLM settings (changed by `triad setup`, `triad key …` or the UI).
#[derive(Clone, Debug)]
pub struct LlmSettings {
    pub api_key: Option<String>,
    pub model: String,
    pub director_model: String,
}

impl LlmSettings {
    pub fn from_env() -> Self {
        let model = env_or("OPENROUTER_MODEL", DEFAULT_MODEL);
        Self {
            api_key: env_opt("OPENROUTER_API_KEY"),
            director_model: env_or("OPENROUTER_DIRECTOR_MODEL", &model),
            model,
        }
    }

    pub fn mock_mode(&self) -> bool {
        self.api_key.is_none()
    }

    /// `sk-or-v1-…c3f2` — enough to recognise a key, never enough to use it.
    pub fn key_hint(&self) -> Option<String> {
        self.api_key.as_ref().map(|k| {
            let n = k.chars().count();
            if n <= 12 {
                return "•".repeat(n);
            }
            let head: String = k.chars().take(9).collect();
            let tail: String = k.chars().skip(n - 4).collect();
            format!("{head}…{tail}")
        })
    }

    /// Persist to `$TRIAD_HOME/config.env` and mirror into the process env so a restart
    /// in the same process (or `Config::from_env`) sees the same values.
    pub fn save(&self) -> Result<PathBuf> {
        let mut pairs = vec![
            ("OPENROUTER_MODEL", self.model.clone()),
            ("OPENROUTER_DIRECTOR_MODEL", self.director_model.clone()),
        ];
        pairs.push(("OPENROUTER_API_KEY", self.api_key.clone().unwrap_or_default()));
        let path = save_config_values(&pairs)?;
        for (k, v) in &pairs {
            if v.is_empty() {
                std::env::remove_var(k);
            } else {
                std::env::set_var(k, v);
            }
        }
        Ok(path)
    }
}

/// Merge key/value pairs into the config file, keeping unknown lines and comments.
pub fn save_config_values(pairs: &[(&str, String)]) -> Result<PathBuf> {
    let path = config_path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let mut updates: BTreeMap<&str, &str> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let mut lines: Vec<String> = Vec::new();
    for line in existing.lines() {
        let trimmed = line.trim_start();
        let key = trimmed.split('=').next().unwrap_or("").trim();
        if !trimmed.starts_with('#') && updates.contains_key(key) {
            if let Some(v) = updates.remove(key) {
                if !v.is_empty() {
                    lines.push(format!("{key}={v}"));
                }
            }
        } else {
            lines.push(line.to_string());
        }
    }
    if lines.is_empty() {
        lines.push("# Triad Canvas — создано `triad setup`. Правь руками или через ⚙ в UI.".into());
    }
    for (k, v) in updates {
        if !v.is_empty() {
            lines.push(format!("{k}={v}"));
        }
    }
    let mut body = lines.join("\n");
    body.push('\n');
    write_private(&path, &body)?;
    Ok(path)
}

fn write_private(path: &Path, body: &str) -> Result<()> {
    fs::write(path, body).with_context(|| format!("cannot write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Read the config file as key/value pairs (for `triad config`).
pub fn read_config_file() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    if let Ok(s) = fs::read_to_string(config_path()) {
        for line in s.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = t.split_once('=') {
                m.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    m
}
