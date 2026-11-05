//! Reference lookups that fill object properties: OpenLibrary (books), TMDB (movies),
//! Wikipedia (people, places, concepts). Returns a normalized card the UI and the agents share.

use anyhow::{Context, Result};
use serde::Serialize;

use super::{http, strip_html};
use crate::{config::LlmSettings, mermaid};

#[derive(Debug, Clone, Serialize, Default)]
pub struct Card {
    pub kind: String,
    pub title: String,
    pub props: serde_json::Map<String, serde_json::Value>,
    pub body: String,
    pub image_url: Option<String>,
    pub url: Option<String>,
    pub source: String,
}

pub async fn lookup(s: &LlmSettings, kind: &str, query: &str) -> Result<Option<Card>> {
    let q = query.trim();
    anyhow::ensure!(!q.is_empty(), "пустой запрос");
    match kind {
        "book" => openlibrary(q).await,
        "movie" => tmdb(s.get("TMDB_API_KEY").context("для фильмов нужен TMDB_API_KEY в настройках")?, q).await,
        _ => wikipedia(q).await,
    }
}

/// Which lookup suits a type.
pub fn kind_for_type(type_id: &str) -> Option<&'static str> {
    match type_id {
        "book" => Some("book"),
        "movie" => Some("movie"),
        "human" | "place" | "idea" | "note" | "page" | "quote" | "event" | "project" => Some("wiki"),
        _ => None,
    }
}

async fn openlibrary(q: &str) -> Result<Option<Card>> {
    let v: serde_json::Value = http()
        .get("https://openlibrary.org/search.json")
        .query(&[("q", q), ("limit", "3"), ("fields", "key,title,author_name,first_publish_year,number_of_pages_median,cover_i,isbn,subject")])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let Some(doc) = v["docs"].as_array().and_then(|a| a.first()) else { return Ok(None) };
    let mut props = serde_json::Map::new();
    if let Some(a) = doc["author_name"].as_array().and_then(|a| a.first()).and_then(|a| a.as_str()) {
        props.insert("author".into(), a.into());
    }
    if let Some(y) = doc["first_publish_year"].as_i64() {
        props.insert("year".into(), y.into());
    }
    if let Some(p) = doc["number_of_pages_median"].as_i64() {
        props.insert("pages".into(), p.into());
    }
    if let Some(isbn) = doc["isbn"].as_array().and_then(|a| a.first()).and_then(|a| a.as_str()) {
        props.insert("isbn".into(), isbn.into());
    }
    let cover = doc["cover_i"].as_i64().map(|c| format!("https://covers.openlibrary.org/b/id/{c}-L.jpg"));
    if let Some(c) = &cover {
        props.insert("cover_url".into(), c.as_str().into());
    }
    let key = doc["key"].as_str().unwrap_or("");
    let url = if key.is_empty() { None } else { Some(format!("https://openlibrary.org{key}")) };
    if let Some(u) = &url {
        props.insert("url".into(), u.as_str().into());
    }
    let subjects = doc["subject"].as_array().map(|a| a.iter().filter_map(|s| s.as_str()).take(6).collect::<Vec<_>>().join(", ")).unwrap_or_default();
    Ok(Some(Card {
        kind: "book".into(),
        title: doc["title"].as_str().unwrap_or(q).to_string(),
        props,
        body: if subjects.is_empty() { String::new() } else { format!("Темы: {subjects}") },
        image_url: cover,
        url,
        source: "OpenLibrary".into(),
    }))
}

async fn tmdb(key: &str, q: &str) -> Result<Option<Card>> {
    let v: serde_json::Value = http()
        .get("https://api.themoviedb.org/3/search/movie")
        .query(&[("api_key", key), ("query", q), ("language", "ru-RU"), ("include_adult", "false")])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let Some(m) = v["results"].as_array().and_then(|a| a.first()) else { return Ok(None) };
    let id = m["id"].as_i64().unwrap_or(0);
    let mut props = serde_json::Map::new();
    if let Some(y) = m["release_date"].as_str().and_then(|d| d.get(0..4)).and_then(|y| y.parse::<i64>().ok()) {
        props.insert("year".into(), y.into());
    }
    let poster = m["poster_path"].as_str().map(|p| format!("https://image.tmdb.org/t/p/w500{p}"));
    if let Some(p) = &poster {
        props.insert("poster_url".into(), p.as_str().into());
    }
    let url = format!("https://www.themoviedb.org/movie/{id}");
    props.insert("url".into(), url.as_str().into());
    // director + runtime from details/credits (best effort)
    if id > 0 {
        if let Ok(d) = http().get(format!("https://api.themoviedb.org/3/movie/{id}")).query(&[("api_key", key), ("language", "ru-RU"), ("append_to_response", "credits")]).send().await {
            if let Ok(d) = d.json::<serde_json::Value>().await {
                if let Some(r) = d["runtime"].as_i64().filter(|r| *r > 0) {
                    props.insert("runtime".into(), r.into());
                }
                if let Some(dir) = d["credits"]["crew"].as_array().and_then(|c| c.iter().find(|p| p["job"].as_str() == Some("Director"))).and_then(|p| p["name"].as_str()) {
                    props.insert("director".into(), dir.into());
                }
            }
        }
    }
    let overview = m["overview"].as_str().unwrap_or("").trim().to_string();
    Ok(Some(Card {
        kind: "movie".into(),
        title: m["title"].as_str().unwrap_or(q).to_string(),
        props,
        body: mermaid::truncate_chars(&overview, 900),
        image_url: poster,
        url: Some(url),
        source: "TMDB".into(),
    }))
}

async fn wikipedia(q: &str) -> Result<Option<Card>> {
    for lang in ["ru", "en"] {
        // 1) search for the best page title, 2) fetch its summary
        let sr: serde_json::Value = http()
            .get(format!("https://{lang}.wikipedia.org/w/api.php"))
            .query(&[("action", "query"), ("list", "search"), ("srsearch", q), ("srlimit", "1"), ("format", "json"), ("utf8", "1")])
            .send()
            .await?
            .json()
            .await
            .unwrap_or_default();
        let Some(title) = sr["query"]["search"].as_array().and_then(|a| a.first()).and_then(|p| p["title"].as_str()) else { continue };
        let sum: serde_json::Value = http()
            .get(format!("https://{lang}.wikipedia.org/api/rest_v1/page/summary/{}", urlenc(title)))
            .send()
            .await?
            .json()
            .await
            .unwrap_or_default();
        let extract = sum["extract"].as_str().unwrap_or("").trim().to_string();
        if extract.is_empty() {
            continue;
        }
        let image = sum["thumbnail"]["source"].as_str().map(str::to_string);
        let url = sum["content_urls"]["desktop"]["page"].as_str().map(str::to_string);
        let mut props = serde_json::Map::new();
        if let Some(d) = sum["description"].as_str() {
            props.insert("role".into(), strip_html(d).into());
        }
        if let Some(u) = &url {
            props.insert("url".into(), u.as_str().into());
        }
        return Ok(Some(Card { kind: "wiki".into(), title: title.to_string(), props, body: mermaid::truncate_chars(&extract, 1200), image_url: image, url, source: format!("Wikipedia ({lang})") }));
    }
    Ok(None)
}

fn urlenc(s: &str) -> String {
    form_urlencoded::byte_serialize(s.replace(' ', "_").as_bytes()).collect()
}

/// The card as prompt text.
pub fn digest(c: &Card) -> String {
    let props = c.props.iter().map(|(k, v)| format!("{k}={}", v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))).collect::<Vec<_>>().join(", ");
    format!("{} ({}): {}{}", c.title, c.source, props, if c.body.is_empty() { String::new() } else { format!(" — {}", c.body) })
}
