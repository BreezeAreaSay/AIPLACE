//! Inputs from the outside world. v1: URLs in the player's line are fetched, boiled down to
//! text, summarised by a small call and stored as a `bookmark` object; the digest is handed
//! to the speakers so they discuss the content, not the link.

use std::time::Duration;

use anyhow::{Context, Result};
use regex::Regex;
use std::sync::OnceLock;

use crate::{
    db, mermaid,
    openrouter::{ChatMessage, ChatOpts},
    state::AppState,
};

pub fn find_urls(text: &str) -> Vec<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#"https?://[^\s<>"'()\[\]]+"#).expect("regex"));
    let mut out: Vec<String> = re.find_iter(text).map(|m| m.as_str().trim_end_matches(['.', ',', ';', '!', '?']).to_string()).collect();
    out.dedup();
    out.truncate(3);
    out
}

/// Very small HTML → text: drop scripts/styles/tags, decode a few entities, squeeze spaces.
pub fn html_to_text(html: &str) -> (Option<String>, String) {
    static TITLE: OnceLock<Regex> = OnceLock::new();
    static DROP: OnceLock<Regex> = OnceLock::new();
    static TAGS: OnceLock<Regex> = OnceLock::new();
    static WS: OnceLock<Regex> = OnceLock::new();
    let title = TITLE
        .get_or_init(|| Regex::new(r"(?is)<title[^>]*>(.*?)</title>").expect("regex"))
        .captures(html)
        .map(|c| c[1].trim().to_string())
        .filter(|t| !t.is_empty());
    let body = DROP
        .get_or_init(|| Regex::new(r"(?is)<script[^>]*>.*?</script>|<style[^>]*>.*?</style>|<noscript[^>]*>.*?</noscript>|<svg[^>]*>.*?</svg>|<nav[^>]*>.*?</nav>|<footer[^>]*>.*?</footer>|<header[^>]*>.*?</header>").expect("regex"))
        .replace_all(html, " ");
    let body = TAGS.get_or_init(|| Regex::new(r"(?s)<br\s*/?>|</p>|</div>|</li>|</h[1-6]>").expect("regex")).replace_all(&body, "\n");
    let body = Regex::new(r"(?s)<[^>]+>").expect("regex").replace_all(&body, " ");
    let body = body
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    let body = WS.get_or_init(|| Regex::new(r"[ \t\r\f\v]+").expect("regex")).replace_all(&body, " ");
    let body = Regex::new(r"\n\s*\n+").expect("regex").replace_all(&body, "\n");
    (title.map(|t| mermaid::truncate_chars(&t, 120)), body.trim().to_string())
}

pub struct Intake {
    pub object_id: String,
    pub title: String,
    pub summary: String,
}

/// Fetch, summarise and file one URL. Errors are soft: a dead link becomes a bookmark with
/// an "unreachable" note so the room can still react.
pub async fn ingest_url(app: &AppState, url: &str) -> Result<Intake> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent("Mozilla/5.0 (compatible; TriadCanvas/0.6; +https://github.com/BreezeAreaSay/AIPLACE)")
        .build()?;
    let fetched = async {
        let resp = client.get(url).send().await.context("fetch")?;
        let ct = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        let bytes = resp.bytes().await.context("body")?;
        let bytes = if bytes.len() > 1_500_000 { bytes.slice(..1_500_000) } else { bytes };
        let raw = String::from_utf8_lossy(&bytes).to_string();
        Ok::<(String, String), anyhow::Error>((ct, raw))
    }
    .await;
    let host = url.split('/').nth(2).unwrap_or("").to_string();
    let (title, text) = match &fetched {
        Ok((ct, raw)) if ct.contains("html") => html_to_text(raw),
        Ok((_, raw)) => (None, raw.clone()),
        Err(_) => (None, String::new()),
    };
    let text = mermaid::truncate_chars(&text, 12_000);
    let fetch_error: Option<String> = fetched.as_ref().err().map(|e| e.to_string());
    let summary = if text.trim().is_empty() {
        format!("Не удалось прочитать страницу ({}).", fetch_error.unwrap_or_else(|| "пусто".to_string()))
    } else {
        let model = app.llm_settings().await.director_model;
        let msgs = [
            ChatMessage::system("Ты — конспектор. Из текста страницы сделай выжимку на русском: 2–4 предложения сути, затем 3–5 конкретных фактов/тезисов списком через «; ». Без вступлений, без markdown."),
            ChatMessage::user(format!("URL: {url}\nЗАГОЛОВОК: {}\nТЕКСТ:\n{text}", title.clone().unwrap_or_default())),
        ];
        let opts = ChatOpts { temperature: 0.3, max_tokens: 500, json_mode: false, prefill: None, presence_penalty: None, frequency_penalty: None };
        match app.llm.chat(&model, &msgs, &opts).await {
            Ok(s) => s.trim().to_string(),
            Err(e) => {
                tracing::warn!(error = %e, "summary failed");
                mermaid::truncate_chars(&text, 600)
            }
        }
    };
    let title = title.unwrap_or_else(|| host.clone());
    let base = mermaid::safe_id(&title);
    let mut id = format!("link_{base}");
    if id.len() > 40 {
        id.truncate(40);
    }
    if db::node_exists(&app.db, &id).await? {
        if let Some(existing) = db::get_node(&app.db, &id).await? {
            if existing.props.0.get("url").and_then(|u| u.as_str()) != Some(url) {
                id = format!("{}_{}", id, chrono::Utc::now().timestamp() % 10_000);
            }
        }
    }
    db::upsert_object(
        &app.db,
        &id,
        &mermaid::sanitize_label(&title),
        "bookmark",
        "user",
        Some(&summary),
        Some(&serde_json::json!({ "url": url, "source": host, "summary": mermaid::truncate_chars(&summary, 300) })),
        None,
        None,
    )
    .await?;
    db::place_unpositioned_nodes(&app.db).await?;
    Ok(Intake { object_id: id, title, summary })
}
