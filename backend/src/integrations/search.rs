//! Web search for the inhabitants. Providers: `ddg` (no key, HTML scrape), `tavily`,
//! `brave`, `searxng`. Plus `fetch`: read a page as text (shares the intake extractor).

use anyhow::{Context, Result};
use serde::Serialize;

use super::{http, strip_html};
use crate::{config::LlmSettings, mermaid};

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

pub async fn search(s: &LlmSettings, query: &str, limit: usize) -> Result<Vec<Hit>> {
    let limit = limit.clamp(1, 10);
    let provider = s.search_provider().context("поиск выключен")?;
    let hits = match provider.as_str() {
        "tavily" => tavily(s.get("SEARCH_API_KEY").context("нужен SEARCH_API_KEY (Tavily)")?, query, limit).await?,
        "brave" => brave(s.get("SEARCH_API_KEY").context("нужен SEARCH_API_KEY (Brave)")?, query, limit).await?,
        "searxng" => searxng(s.get("SEARXNG_URL").context("нужен SEARXNG_URL")?, query, limit).await?,
        _ => ddg(query, limit).await?,
    };
    Ok(hits.into_iter().take(limit).collect())
}

async fn tavily(key: &str, q: &str, n: usize) -> Result<Vec<Hit>> {
    let v: serde_json::Value = http()
        .post("https://api.tavily.com/search")
        .json(&serde_json::json!({ "api_key": key, "query": q, "max_results": n, "search_depth": "basic" }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(v["results"]
        .as_array()
        .map(|a| a.iter().map(|r| Hit { title: r["title"].as_str().unwrap_or("").into(), url: r["url"].as_str().unwrap_or("").into(), snippet: mermaid::truncate_chars(r["content"].as_str().unwrap_or(""), 300) }).collect())
        .unwrap_or_default())
}

async fn brave(key: &str, q: &str, n: usize) -> Result<Vec<Hit>> {
    let v: serde_json::Value = http()
        .get("https://api.search.brave.com/res/v1/web/search")
        .query(&[("q", q), ("count", &n.to_string()), ("search_lang", "ru")])
        .header("X-Subscription-Token", key)
        .header("Accept", "application/json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(v["web"]["results"]
        .as_array()
        .map(|a| a.iter().map(|r| Hit { title: strip_html(r["title"].as_str().unwrap_or("")), url: r["url"].as_str().unwrap_or("").into(), snippet: mermaid::truncate_chars(&strip_html(r["description"].as_str().unwrap_or("")), 300) }).collect())
        .unwrap_or_default())
}

async fn searxng(base: &str, q: &str, n: usize) -> Result<Vec<Hit>> {
    let v: serde_json::Value = http()
        .get(format!("{}/search", base.trim_end_matches('/')))
        .query(&[("q", q), ("format", "json"), ("language", "ru-RU")])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(v["results"]
        .as_array()
        .map(|a| a.iter().take(n).map(|r| Hit { title: r["title"].as_str().unwrap_or("").into(), url: r["url"].as_str().unwrap_or("").into(), snippet: mermaid::truncate_chars(r["content"].as_str().unwrap_or(""), 300) }).collect())
        .unwrap_or_default())
}

/// DuckDuckGo's HTML endpoint: no key, no JSON, good enough for "what does the room know".
async fn ddg(q: &str, n: usize) -> Result<Vec<Hit>> {
    let html = http()
        .post("https://html.duckduckgo.com/html/")
        .form(&[("q", q), ("kl", "ru-ru")])
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(parse_ddg(&html, n))
}

fn parse_ddg(html: &str, n: usize) -> Vec<Hit> {
    let re = regex::Regex::new(r#"(?s)<a[^>]+class="result__a"[^>]+href="([^"]+)"[^>]*>(.*?)</a>.*?(?:<a[^>]+class="result__snippet"[^>]*>(.*?)</a>|<div[^>]+class="result__snippet"[^>]*>(.*?)</div>)"#).unwrap();
    re.captures_iter(html)
        .filter_map(|c| {
            let raw = c.get(1)?.as_str();
            let url = decode_ddg_url(raw);
            if url.contains("duckduckgo.com/y.js") {
                return None;
            }
            Some(Hit {
                title: strip_html(c.get(2).map(|m| m.as_str()).unwrap_or("")),
                url,
                snippet: mermaid::truncate_chars(&strip_html(c.get(3).or_else(|| c.get(4)).map(|m| m.as_str()).unwrap_or("")), 300),
            })
        })
        .take(n)
        .collect()
}

fn decode_ddg_url(raw: &str) -> String {
    // //duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com&rut=…
    if let Some(i) = raw.find("uddg=") {
        let enc = &raw[i + 5..];
        let enc = enc.split('&').next().unwrap_or(enc);
        return form_urlencoded::parse(format!("u={enc}").as_bytes()).next().map(|(_, v)| v.to_string()).unwrap_or_else(|| raw.to_string());
    }
    if raw.starts_with("//") {
        return format!("https:{raw}");
    }
    raw.to_string()
}

/// Read a web page as text (title + body), capped for prompts.
pub async fn fetch(url: &str, max_chars: usize) -> Result<(String, String)> {
    anyhow::ensure!(url.starts_with("http://") || url.starts_with("https://"), "нужен http(s) URL");
    let res = http().get(url).header("Accept", "text/html,application/xhtml+xml,text/plain;q=0.9,*/*;q=0.5").send().await?.error_for_status()?;
    let ct = res.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let bytes = res.bytes().await?;
    let bytes = &bytes[..bytes.len().min(2 * 1024 * 1024)];
    if ct.contains("pdf") || url.to_lowercase().ends_with(".pdf") {
        let text = pdf_extract::extract_text_from_mem(bytes).unwrap_or_default();
        return Ok((url.to_string(), mermaid::truncate_chars(text.trim(), max_chars)));
    }
    let body = String::from_utf8_lossy(bytes);
    if ct.contains("html") || body.trim_start().starts_with('<') {
        let (title, text) = crate::intake::html_to_text(&body);
        Ok((title.unwrap_or_else(|| url.to_string()), mermaid::truncate_chars(text.trim(), max_chars)))
    } else {
        Ok((url.to_string(), mermaid::truncate_chars(body.trim(), max_chars)))
    }
}

pub fn digest(hits: &[Hit]) -> String {
    if hits.is_empty() {
        return "в сети ничего не нашлось".into();
    }
    hits.iter().map(|h| format!("- {} — {} ({})", h.title, h.snippet, h.url)).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ddg_results() {
        let html = r#"<div class="result"><h2><a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fa&amp;rut=1">Example <b>A</b></a></h2><a class="result__snippet" href="x">Snippet &amp; more</a></div>"#;
        let hits = parse_ddg(html, 5);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://example.com/a");
        assert_eq!(hits[0].title, "Example A");
        assert_eq!(hits[0].snippet, "Snippet & more");
    }
}
