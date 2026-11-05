//! RSS / Atom: objects of type `feed` are polled by the heartbeat; new items become
//! bookmarks (source = the feed) linked to the feed object, so the inhabitants can bring
//! fresh news into the room and you can read them in the library.

use anyhow::Result;
use serde::Serialize;

use super::{http, strip_html};
use crate::{db, mermaid, state::AppState};

#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub title: String,
    pub link: String,
    pub summary: String,
    pub published: String,
}

fn tag(block: &str, names: &[&str]) -> Option<String> {
    for n in names {
        let re = regex::Regex::new(&format!(r"(?s)<{n}(?:\s[^>]*)?>(.*?)</{n}>")).ok()?;
        if let Some(c) = re.captures(block) {
            let raw = c.get(1)?.as_str();
            let raw = raw.trim().strip_prefix("<![CDATA[").and_then(|s| s.strip_suffix("]]>")).unwrap_or(raw.trim());
            let v = strip_html(raw);
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

fn atom_link(block: &str) -> Option<String> {
    let re = regex::Regex::new(r#"<link[^>]*?href="([^"]+)"[^>]*?/?>"#).ok()?;
    // prefer rel="alternate" or no rel
    for c in re.captures_iter(block) {
        let whole = c.get(0)?.as_str();
        if !whole.contains("rel=") || whole.contains(r#"rel="alternate""#) {
            return Some(c.get(1)?.as_str().to_string());
        }
    }
    re.captures(block).and_then(|c| c.get(1)).map(|m| m.as_str().to_string())
}

pub fn parse(xml: &str, limit: usize) -> (String, Vec<Item>) {
    let is_atom = xml.contains("<feed") && xml.contains("<entry");
    let (item_tag, title_tags): (&str, &[&str]) = if is_atom { ("entry", &["title"]) } else { ("item", &["title"]) };
    let head = xml.split(&format!("<{item_tag}")).next().unwrap_or("");
    let feed_title = tag(head, &["title"]).unwrap_or_default();
    let mut items = vec![];
    for block in xml.split(&format!("<{item_tag}")).skip(1) {
        let block = block.split(&format!("</{item_tag}>")).next().unwrap_or("");
        let title = tag(block, title_tags).unwrap_or_else(|| "(без названия)".into());
        let link = if is_atom { atom_link(block) } else { tag(block, &["link", "guid"]) }.unwrap_or_default();
        if link.is_empty() {
            continue;
        }
        let summary = tag(block, &["description", "summary", "content", "content:encoded"]).map(|s| mermaid::truncate_chars(&s, 400)).unwrap_or_default();
        let published = tag(block, &["pubDate", "published", "updated", "dc:date"]).unwrap_or_default();
        items.push(Item { title: mermaid::truncate_chars(&title, 160), link, summary, published });
        if items.len() >= limit {
            break;
        }
    }
    (feed_title, items)
}

pub async fn fetch(url: &str, limit: usize) -> Result<(String, Vec<Item>)> {
    anyhow::ensure!(url.starts_with("http"), "нужен http(s) URL ленты");
    let xml = http().get(url).header("Accept", "application/rss+xml, application/atom+xml, application/xml, text/xml;q=0.9, */*;q=0.5").send().await?.error_for_status()?.text().await?;
    Ok(parse(&xml, limit.clamp(1, 30)))
}

fn host(url: &str) -> String {
    url.split("//").nth(1).unwrap_or(url).split('/').next().unwrap_or("").trim_start_matches("www.").to_string()
}

fn item_id(link: &str) -> String {
    use sha2::{Digest, Sha256};
    let h = hex::encode(Sha256::digest(link.as_bytes()));
    format!("feed_{}_{}", mermaid::safe_id(&host(link)).chars().take(16).collect::<String>(), &h[..10])
}

/// Poll every active feed object; returns how many bookmarks were created.
pub async fn poll(app: &AppState) -> Result<usize> {
    let nodes = db::list_nodes(&app.db).await?;
    let feeds: Vec<_> = nodes.iter().filter(|n| n.type_id == "feed" && n.archived == 0 && n.props.0.get("active").and_then(|a| a.as_bool()).unwrap_or(true)).cloned().collect();
    let mut created = 0;
    for f in feeds {
        let Some(url) = f.props.0.get("url").and_then(|u| u.as_str()).filter(|u| u.starts_with("http")) else { continue };
        let (title, items) = match fetch(url, 8).await {
            Ok(x) => x,
            Err(e) => {
                tracing::warn!(feed = %f.label, error = %e, "feed fetch failed");
                continue;
            }
        };
        let owner = f.props.0.get("owner").and_then(|o| o.as_str()).unwrap_or("archivist").to_string();
        let mut new_here = 0;
        for it in items.iter().take(3) {
            let id = item_id(&it.link);
            if db::node_exists(&app.db, &id).await? {
                continue;
            }
            let props = serde_json::json!({ "url": it.link, "source": if title.is_empty() { f.label.clone() } else { title.clone() }, "summary": it.summary, "read": false, "tags": ["лента"] });
            db::upsert_object(&app.db, &id, &mermaid::sanitize_label(&it.title), "bookmark", &owner, Some(&format!("{}\n\n_{}_", it.summary, it.published)), Some(&props), None, None).await?;
            db::insert_relation(&app.db, &id, &f.id, "about", "из ленты", "system", 1.0).await?;
            new_here += 1;
        }
        created += new_here;
        let mut p = f.props.0.clone();
        p["last_fetch"] = serde_json::Value::String(format!("{} · +{new_here}", chrono::Local::now().format("%d.%m %H:%M")));
        db::update_object_meta(&app.db, &f.id, None, Some(&p), None).await?;
    }
    if created > 0 {
        db::place_unpositioned_nodes(&app.db).await?;
    }
    Ok(created)
}

/// Fresh, not yet discussed feed bookmarks (for the heartbeat's "news" beat).
pub async fn fresh_news(app: &AppState, limit: usize) -> Result<Vec<crate::models::GraphNode>> {
    let nodes = db::list_nodes(&app.db).await?;
    let cutoff = (chrono::Utc::now() - chrono::Duration::hours(48)).to_rfc3339();
    let mut v: Vec<_> = nodes
        .into_iter()
        .filter(|n| n.type_id == "bookmark" && n.id.starts_with("feed_") && n.archived == 0 && n.created_at > cutoff && n.props.0.get("discussed").and_then(|d| d.as_bool()) != Some(true))
        .collect();
    v.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    v.truncate(limit);
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rss_and_atom() {
        let rss = "<rss><channel><title>Лента</title><item><title><![CDATA[Первая & новость]]></title><link>https://ex.com/1</link><description><p>Текст <b>жирный</b></p></description><pubDate>Mon</pubDate></item></channel></rss>";
        let (t, items) = parse(rss, 5);
        assert_eq!(t, "Лента");
        assert_eq!(items[0].title, "Первая & новость");
        assert_eq!(items[0].summary, "Текст жирный");
        let atom = r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>A</title><entry><title>E1</title><link rel="alternate" href="https://ex.com/e1"/><summary>s</summary><updated>2026</updated></entry></feed>"#;
        let (_, items) = parse(atom, 5);
        assert_eq!(items[0].link, "https://ex.com/e1");
    }
}
