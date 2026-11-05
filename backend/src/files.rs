//! Attachments: any file dropped on an object. Stored on disk under `files_dir()/<room>/`,
//! described in SQLite, and — when it is text, markdown, CSV, JSON, HTML or PDF — read into
//! `attachments.text` so the inhabitants can actually use what you gave them.

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::{config, db, mermaid, models::Attachment, state::AppState};

const MAX_TEXT: usize = 120_000;

fn safe_name(name: &str) -> String {
    let n: String = name
        .chars()
        .rev()
        .take(80)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') { c } else { '_' })
        .collect();
    let n = n.trim().trim_matches('.').to_string();
    if n.is_empty() {
        "file".into()
    } else {
        n
    }
}

pub fn room_dir(room_id: &str) -> PathBuf {
    config::files_dir().join(mermaid::safe_id(room_id))
}

pub fn guess_mime(name: &str, declared: Option<&str>) -> String {
    match declared.map(str::trim).filter(|m| !m.is_empty() && *m != "application/octet-stream") {
        Some(m) => m.to_string(),
        None => mime_guess::from_path(name).first_or_octet_stream().to_string(),
    }
}

pub fn is_image(mime: &str) -> bool {
    mime.starts_with("image/")
}

/// Pull plain text out of what we can read. Images and unknown binaries yield "".
pub fn extract_text(name: &str, mime: &str, bytes: &[u8]) -> String {
    let lower = name.to_lowercase();
    let text = if mime == "application/pdf" || lower.ends_with(".pdf") {
        pdf_extract::extract_text_from_mem(bytes).unwrap_or_default()
    } else if mime.starts_with("text/html") || lower.ends_with(".html") || lower.ends_with(".htm") {
        crate::intake::html_to_text(&String::from_utf8_lossy(bytes)).1
    } else if mime.starts_with("text/")
        || matches!(mime, "application/json" | "application/xml" | "application/x-yaml" | "application/toml" | "application/javascript" | "application/x-sh")
        || [".md", ".markdown", ".txt", ".csv", ".tsv", ".json", ".xml", ".yaml", ".yml", ".toml", ".log", ".srt", ".vtt", ".py", ".rs", ".ts", ".tsx", ".js", ".sql", ".ini", ".cfg", ".tex", ".org", ".ics"].iter().any(|e| lower.ends_with(e))
    {
        String::from_utf8_lossy(bytes).to_string()
    } else {
        String::new()
    };
    let text = text.replace('\0', "").replace("\r\n", "\n");
    let collapsed: String = {
        // squash runs of blank lines PDFs love to produce
        let mut out = String::with_capacity(text.len());
        let mut blank = 0;
        for line in text.lines() {
            if line.trim().is_empty() {
                blank += 1;
                if blank <= 1 {
                    out.push('\n');
                }
            } else {
                blank = 0;
                out.push_str(line.trim_end());
                out.push('\n');
            }
        }
        out
    };
    mermaid::truncate_chars(collapsed.trim(), MAX_TEXT)
}

/// Save bytes as an attachment of `node_id`.
pub async fn save(app: &AppState, node_id: &str, name: &str, declared_mime: Option<&str>, bytes: &[u8], actor: &str) -> Result<Attachment> {
    anyhow::ensure!(db::node_exists(&app.db, node_id).await?, "нет такого объекта");
    anyhow::ensure!(bytes.len() <= 50 * 1024 * 1024, "файл больше 50 МБ");
    let dir = room_dir(&app.room_id);
    tokio::fs::create_dir_all(&dir).await.with_context(|| format!("cannot create {}", dir.display()))?;
    let id = format!("{:x}{:04x}", chrono::Utc::now().timestamp_millis(), rand::random::<u16>());
    let fname = format!("{id}_{}", safe_name(name));
    let path = dir.join(&fname);
    tokio::fs::write(&path, bytes).await.with_context(|| format!("cannot write {}", path.display()))?;
    let mime = guess_mime(name, declared_mime);
    let mut text = if is_image(&mime) { String::new() } else { extract_text(name, &mime, bytes) };
    if text.is_empty() && mime.starts_with("audio/") {
        let s = app.llm_settings().await;
        if s.stt_enabled() {
            match crate::integrations::speech::stt(&s, bytes.to_vec(), name, &mime).await {
                Ok(t) if !t.trim().is_empty() => text = format!("Расшифровка «{name}»:\n\n{t}"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "audio transcription failed"),
            }
        }
    }
    let a = Attachment {
        id,
        node_id: node_id.to_string(),
        name: safe_name(name),
        mime,
        size: bytes.len() as i64,
        path: path.to_string_lossy().to_string(),
        text_len: text.chars().count() as i64,
        text,
        created_by: actor.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    db::insert_attachment(&app.db, &a).await?;
    Ok(a)
}

pub async fn remove(app: &AppState, id: &str) -> Result<Option<Attachment>> {
    let Some(a) = db::get_attachment(&app.db, id).await? else { return Ok(None) };
    db::delete_attachment(&app.db, id).await?;
    let _ = tokio::fs::remove_file(&a.path).await;
    Ok(Some(a))
}

/// A short "what's attached" line for prompts: names, kinds, and the start of the text.
pub fn digest(atts: &[Attachment], per_file_chars: usize) -> String {
    atts.iter()
        .map(|a| {
            let kind = if is_image(&a.mime) { "картинка" } else if a.text_len > 0 { "документ" } else { "файл" };
            let excerpt = if a.text.trim().is_empty() { String::new() } else { format!(": {}", mermaid::truncate_chars(&a.text.split_whitespace().collect::<Vec<_>>().join(" "), per_file_chars)) };
            format!("  📎 {} ({kind}, {}{}){excerpt}", a.name, human_size(a.size), if a.text_len > 0 { format!(", {} симв. текста", a.text_len) } else { String::new() })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn human_size(n: i64) -> String {
    if n < 1024 {
        format!("{n} Б")
    } else if n < 1024 * 1024 {
        format!("{:.0} КБ", n as f64 / 1024.0)
    } else {
        format!("{:.1} МБ", n as f64 / 1024.0 / 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_text_and_squashes_blank_lines() {
        let t = extract_text("a.txt", "text/plain", b"one\r\n\r\n\r\n\r\ntwo  \n");
        assert_eq!(t, "one\n\ntwo");
        assert_eq!(extract_text("x.png", "image/png", b"\x89PNG"), "");
    }

    #[test]
    fn safe_names() {
        assert_eq!(safe_name("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(safe_name("Отчёт Q3 (final).pdf"), "Отчёт Q3 _final_.pdf");
    }
}
