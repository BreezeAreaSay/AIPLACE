//! The room's windows to the world: web search, reference lookups, weather, calendar,
//! RSS feeds, GitHub and speech. Each one is optional and lights up from Settings; all of them
//! are exposed to the inhabitants through `tools.rs` and to you through the UI.

pub mod calendar;
pub mod feeds;
pub mod github;
pub mod lookup;
pub mod search;
pub mod speech;
pub mod weather;

use std::{sync::OnceLock, time::Duration};

pub fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(format!("TriadCanvas/{} (+https://github.com/BreezeAreaSay/AIPLACE)", crate::update::VERSION))
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .expect("http client")
    })
}

/// Strip tags, decode the handful of entities feeds use, squash whitespace.
pub fn strip_html(s: &str) -> String {
    let no_tags = regex::Regex::new(r"(?s)<[^>]+>").unwrap().replace_all(s, " ");
    let decoded = no_tags
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&laquo;", "«")
        .replace("&raquo;", "»")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Features currently live, from the hot settings.
pub fn features(s: &crate::config::LlmSettings) -> crate::models::Features {
    crate::models::Features {
        search: s.search_provider().is_some(),
        weather: s.get("WEATHER_CITY").is_some(),
        calendar: s.get("CALENDAR_ICS_URL").is_some(),
        tmdb: s.get("TMDB_API_KEY").is_some(),
        github: true,
        tts: s.tts_enabled(),
        stt: s.stt_enabled(),
    }
}

/// "За окном" line (weather + local time) when a city is configured; cached by the weather module.
pub async fn world_line(app: &crate::state::AppState) -> Option<String> {
    let s = app.llm_settings().await;
    let city = s.get("WEATHER_CITY")?;
    match weather::current(city).await {
        Ok(w) => Some(weather::line(&w)),
        Err(e) => {
            tracing::debug!(error = %e, "weather unavailable");
            None
        }
    }
}

/// Short calendar digest for the Caster (next 3 days), empty when nothing is configured and
/// the library has no dated items.
pub async fn calendar_line(app: &crate::state::AppState) -> Option<String> {
    let ev = calendar::upcoming(app, 3).await.ok()?;
    if ev.is_empty() {
        return None;
    }
    Some(calendar::digest(&ev[..ev.len().min(6)]))
}
