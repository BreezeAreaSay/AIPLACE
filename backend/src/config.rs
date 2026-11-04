//! Runtime configuration loaded from environment / `.env`.

use anyhow::Context;

#[derive(Clone, Debug)]
pub struct Config {
    pub openrouter_api_key: Option<String>,
    pub openrouter_model: String,
    pub director_model: String,
    pub database_url: String,
    pub host: String,
    pub port: u16,
    pub static_dir: String,
    pub app_url: String,
    pub context_messages: usize,
    pub tick_entropy_hp: i64,
    pub tick_entropy_dopamine: i64,
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_string())
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let api_key = std::env::var("OPENROUTER_API_KEY")
            .ok()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty());
        let model = env_or("OPENROUTER_MODEL", "deepseek/deepseek-chat");
        let director_model = env_or("OPENROUTER_DIRECTOR_MODEL", &model);
        let port: u16 = env_or("PORT", "3000")
            .parse()
            .context("PORT must be a number")?;
        Ok(Self {
            openrouter_api_key: api_key,
            openrouter_model: model,
            director_model,
            database_url: env_or("DATABASE_URL", "sqlite://triad.db?mode=rwc"),
            host: env_or("HOST", "0.0.0.0"),
            port,
            static_dir: env_or("STATIC_DIR", "../frontend/dist"),
            app_url: env_or("APP_URL", "http://localhost:3000"),
            context_messages: env_or("CONTEXT_MESSAGES", "14").parse().unwrap_or(14),
            tick_entropy_hp: env_or("TICK_ENTROPY_HP", "2").parse().unwrap_or(2),
            tick_entropy_dopamine: env_or("TICK_ENTROPY_DOPAMINE", "3").parse().unwrap_or(3),
        })
    }

    /// No API key → the built-in mock LLM answers, so the room still runs offline.
    pub fn mock_mode(&self) -> bool {
        self.openrouter_api_key.is_none()
    }
}
