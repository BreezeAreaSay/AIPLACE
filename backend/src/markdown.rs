//! Markdown as the library's lingua franca: notes are `.md` with YAML-ish frontmatter and
//! `[[wikilinks]]`, so a folder of Obsidian notes drops straight in and comes straight out.

use std::collections::BTreeMap;

use crate::models::{GraphEdge, GraphNode, NodeNote, ObjectType};

/// A parsed `.md` file: frontmatter keys, title and body with the frontmatter and title heading removed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MdDoc {
    pub front: BTreeMap<String, serde_json::Value>,
    pub title: String,
    pub body: String,
}

fn parse_scalar(raw: &str) -> serde_json::Value {
    let v = raw.trim().trim_matches('"').trim_matches('\'').trim();
    if v.is_empty() {
        return serde_json::Value::Null;
    }
    match v {
        "true" | "yes" | "да" => return serde_json::Value::Bool(true),
        "false" | "no" | "нет" => return serde_json::Value::Bool(false),
        _ => {}
    }
    if let Ok(n) = v.parse::<f64>() {
        if let Some(num) = serde_json::Number::from_f64(n) {
            return serde_json::Value::Number(num);
        }
    }
    if v.starts_with('[') && v.ends_with(']') {
        let items: Vec<serde_json::Value> = v[1..v.len() - 1]
            .split(',')
            .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|s| !s.is_empty())
            .map(serde_json::Value::String)
            .collect();
        return serde_json::Value::Array(items);
    }
    serde_json::Value::String(v.to_string())
}

/// Parse `---\nkey: value\n---` frontmatter (flat keys only, plus `- item` lists), a leading `# Title`,
/// and the rest as body. `fallback_title` is normally the file name without extension.
pub fn parse(content: &str, fallback_title: &str) -> MdDoc {
    let content = content.replace("\r\n", "\n");
    let mut front = BTreeMap::new();
    let mut rest = content.as_str();
    if let Some(after) = rest.strip_prefix("---\n") {
        if let Some(end) = after.find("\n---") {
            let block = &after[..end];
            let mut last_key: Option<String> = None;
            for line in block.lines() {
                if let Some(item) = line.strip_prefix("- ").or_else(|| line.strip_prefix("  - ")) {
                    if let Some(k) = &last_key {
                        let entry = front.entry(k.clone()).or_insert_with(|| serde_json::Value::Array(vec![]));
                        if !entry.is_array() {
                            *entry = serde_json::Value::Array(vec![]);
                        }
                        entry.as_array_mut().unwrap().push(serde_json::Value::String(item.trim().trim_matches('"').to_string()));
                    }
                    continue;
                }
                if let Some((k, v)) = line.split_once(':') {
                    let key = k.trim().to_lowercase();
                    if key.is_empty() || key.contains(' ') {
                        continue;
                    }
                    front.insert(key.clone(), parse_scalar(v));
                    last_key = Some(key);
                }
            }
            rest = after[end + 4..].trim_start_matches('\n');
        }
    }
    let mut body = rest.trim().to_string();
    let mut title = front.get("title").and_then(|t| t.as_str()).map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    if let Some(first) = body.lines().next() {
        if let Some(h) = first.strip_prefix("# ") {
            if title.is_none() {
                title = Some(h.trim().to_string());
            }
            body = body[first.len()..].trim_start().to_string();
        }
    }
    MdDoc { front, title: title.unwrap_or_else(|| fallback_title.trim().to_string()), body }
}

/// `[[Target]]`, `[[Target|alias]]`, `[[Target#heading]]` → distinct targets in order of appearance.
pub fn wikilinks(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else { break };
        let inner = &after[..end];
        let target = inner.split(['|', '#']).next().unwrap_or("").trim();
        if !target.is_empty() && !out.iter().any(|t| t.eq_ignore_ascii_case(target)) {
            out.push(target.to_string());
        }
        rest = &after[end + 2..];
    }
    out
}

fn yaml_scalar(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => {
            if s.contains(':') || s.contains('#') || s.starts_with('[') {
                format!("\"{}\"", s.replace('"', "\\\""))
            } else {
                s.clone()
            }
        }
        serde_json::Value::Array(items) => format!("[{}]", items.iter().map(yaml_scalar).collect::<Vec<_>>().join(", ")),
        other => other.to_string(),
    }
}

/// One object as a standalone `.md` with frontmatter, body, notes and wikilinked relations.
pub fn render_note(node: &GraphNode, ty: Option<&ObjectType>, notes: &[NodeNote], edges: &[GraphEdge], all: &[GraphNode], name: &dyn Fn(&str) -> String) -> String {
    let mut out = String::from("---\n");
    out.push_str(&format!("title: {}\n", yaml_scalar(&serde_json::Value::String(node.label.clone()))));
    out.push_str(&format!("type: {}\n", node.type_id));
    out.push_str(&format!("id: {}\n", node.id));
    out.push_str(&format!("author: {}\n", name(&node.created_by)));
    out.push_str(&format!("created: {}\n", node.created_at));
    out.push_str(&format!("updated: {}\n", node.updated_at));
    if node.pinned != 0 {
        out.push_str("pinned: true\n");
    }
    if node.archived != 0 {
        out.push_str("archived: true\n");
    }
    if let Some(obj) = node.props.0.as_object() {
        for (k, v) in obj {
            if v.is_null() {
                continue;
            }
            out.push_str(&format!("{k}: {}\n", yaml_scalar(v)));
        }
    }
    out.push_str("---\n\n");
    out.push_str(&format!("# {}\n\n", node.label));
    if let Some(t) = ty {
        out.push_str(&format!("_{} {}_\n\n", t.icon, t.name));
    }
    if !node.body.trim().is_empty() {
        out.push_str(node.body.trim());
        out.push_str("\n\n");
    }
    let mine: Vec<&NodeNote> = notes.iter().filter(|n| n.node_id == node.id).collect();
    if !mine.is_empty() {
        out.push_str("## Пометки\n\n");
        for n in mine {
            out.push_str(&format!("- **{}**: {}\n", name(&n.author), n.text));
        }
        out.push('\n');
    }
    let links: Vec<String> = edges
        .iter()
        .filter(|e| e.source_id == node.id || e.target_id == node.id)
        .map(|e| {
            let (arrow, other) = if e.source_id == node.id { ("→", &e.target_id) } else { ("←", &e.source_id) };
            let label = all.iter().find(|n| &n.id == other).map(|n| n.label.as_str()).unwrap_or(other.as_str());
            let rel = if e.relation_label.is_empty() { e.kind.clone() } else { e.relation_label.clone() };
            format!("- {arrow} _{rel}_ [[{label}]]")
        })
        .collect();
    if !links.is_empty() {
        out.push_str("## Связи\n\n");
        out.push_str(&links.join("\n"));
        out.push('\n');
    }
    out
}

/// Append `text` to a body, optionally under a `## heading` (reused if already present at the end).
pub fn append_section(body: &str, heading: Option<&str>, text: &str) -> String {
    let mut out = body.trim_end().to_string();
    if let Some(h) = heading.map(str::trim).filter(|h| !h.is_empty()) {
        let needle = format!("## {h}");
        if !out.lines().any(|l| l.trim() == needle) {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(&needle);
        }
    }
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(text.trim());
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frontmatter_title_and_body() {
        let d = parse("---\ntype: task\ndue: 2026-11-08\ndone: true\ntags:\n- a\n- b\n---\n# Купить лампу\n\nТекст [[Осциллограф С1-94|осциллограф]] и [[Иван]].\n", "file");
        assert_eq!(d.title, "Купить лампу");
        assert_eq!(d.front.get("type").and_then(|v| v.as_str()), Some("task"));
        assert_eq!(d.front.get("done"), Some(&serde_json::Value::Bool(true)));
        assert_eq!(d.front.get("tags").and_then(|v| v.as_array()).map(|a| a.len()), Some(2));
        assert!(d.body.starts_with("Текст"));
        assert_eq!(wikilinks(&d.body), vec!["Осциллограф С1-94", "Иван"]);
    }

    #[test]
    fn falls_back_to_filename() {
        let d = parse("just text", "Заметка из файла");
        assert_eq!(d.title, "Заметка из файла");
        assert_eq!(d.body, "just text");
        assert!(d.front.is_empty());
    }

    #[test]
    fn appends_under_heading_once() {
        let a = append_section("intro", Some("Лог"), "первая");
        let b = append_section(&a, Some("Лог"), "вторая");
        assert_eq!(b.matches("## Лог").count(), 1);
        assert!(b.ends_with("вторая\n"));
    }
}
