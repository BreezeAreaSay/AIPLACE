//! Mermaid.js helpers: id/label sanitization and live block-schema generation.

use regex::Regex;
use std::sync::OnceLock;

use crate::models::{GraphEdge, GraphNode, NODE_TYPES};

pub const CLASS_DEFS: &str = "\
classDef physicist stroke:#00F0FF,stroke-width:2px,fill:#0D0E12,color:#fff;
classDef lit_teacher stroke:#FF0055,stroke-width:2px,fill:#0D0E12,color:#fff;
classDef graffiti stroke:#CCFF00,stroke-width:2px,fill:#0D0E12,color:#fff;
classDef user stroke:#ffffff,stroke-width:2px,fill:#1a1b26,color:#fff;";

pub const AUTHORS: [&str; 4] = ["physicist", "lit_teacher", "graffiti", "user"];

fn re(cell: &'static OnceLock<Regex>, pat: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pat).expect("static regex"))
}

/// Turn any string into a safe Mermaid node id (`[A-Za-z0-9_]`, not starting with a digit).
/// Cyrillic is transliterated so ids stay ASCII and stable across rounds.
pub fn safe_id(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.trim().chars() {
        match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' => out.push(ch),
            ' ' | '-' | '.' | '/' | ':' => out.push('_'),
            c => out.push_str(&translit(c)),
        }
    }
    static DUP: OnceLock<Regex> = OnceLock::new();
    let mut out = re(&DUP, "_+").replace_all(&out, "_").trim_matches('_').to_lowercase();
    if out.is_empty() {
        out = "node".into();
    }
    if out.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        out.insert_str(0, "n_");
    }
    if out.len() > 40 {
        out.truncate(40);
    }
    // reserved words that break `graph TD`
    if matches!(out.as_str(), "end" | "graph" | "subgraph" | "class" | "style" | "click") {
        out.push_str("_n");
    }
    out
}

fn translit(c: char) -> String {
    let s = match c.to_lowercase().next().unwrap_or(c) {
        'а' => "a", 'б' => "b", 'в' => "v", 'г' => "g", 'д' => "d", 'е' | 'ё' => "e", 'ж' => "zh",
        'з' => "z", 'и' | 'й' => "i", 'к' => "k", 'л' => "l", 'м' => "m", 'н' => "n", 'о' => "o",
        'п' => "p", 'р' => "r", 'с' => "s", 'т' => "t", 'у' => "u", 'ф' => "f", 'х' => "h",
        'ц' => "c", 'ч' => "ch", 'ш' => "sh", 'щ' => "sch", 'ы' => "y", 'э' => "e", 'ю' => "yu",
        'я' => "ya", 'ь' | 'ъ' => "",
        _ => "",
    };
    s.to_string()
}

/// Strip everything that breaks Mermaid label syntax; collapse whitespace; cap length.
pub fn sanitize_label(raw: &str) -> String {
    static BAD: OnceLock<Regex> = OnceLock::new();
    static WS: OnceLock<Regex> = OnceLock::new();
    let s = re(&BAD, r#"["'`\[\]\{\}\(\)<>|#;\\]"#).replace_all(raw, " ");
    let s = re(&WS, r"\s+").replace_all(&s, " ").trim().to_string();
    let s = if s.is_empty() { "…".to_string() } else { s };
    truncate_chars(&s, 60)
}

pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max - 1).collect();
    t.push('…');
    t
}

pub fn normalize_node_type(raw: &str) -> &'static str {
    let r = raw.trim().to_lowercase();
    NODE_TYPES.iter().copied().find(|t| *t == r).unwrap_or(match r.as_str() {
        "problem" | "question" | "idea" | "topic" => "concept",
        "action" | "todo" | "step" | "plan" => "task",
        "argument" | "objection" | "clash" | "tension" => "conflict",
        "conclusion" | "thesis" | "lesson" => "insight",
        _ => "concept",
    })
}

pub fn normalize_author(raw: &str) -> &'static str {
    let r = raw.trim().to_lowercase();
    AUTHORS.iter().copied().find(|a| *a == r).unwrap_or(match r.as_str() {
        "arkady" | "аркадий" | "физик" => "physicist",
        "sofia" | "sofya" | "софья" | "литератор" => "lit_teacher",
        "kep" | "cap" | "кэп" | "кислота" => "graffiti",
        _ => "user",
    })
}

fn shape(node_type: &str, label: &str) -> String {
    match node_type {
        "task" => format!("[[\"{label}\"]]"),
        "conflict" => format!("{{{{\"{label}\"}}}}"),
        "insight" => format!("([\"{label}\"])"),
        _ => format!("[\"{label}\"]"),
    }
}

/// Render the stored knowledge graph as a `graph TD` flowchart, color-coded by author.
pub fn build_graph(nodes: &[GraphNode], edges: &[GraphEdge]) -> String {
    let mut out = String::from("graph TD\n");
    if nodes.is_empty() {
        out.push_str("  empty[\"Холст пуст. Скажи что-нибудь в комнату.\"]:::user\n");
        out.push_str(CLASS_DEFS);
        return out;
    }
    for n in nodes {
        let id = safe_id(&n.id);
        let label = sanitize_label(&n.label);
        let cls = normalize_author(&n.created_by);
        out.push_str(&format!(
            "  {id}{}:::{cls}\n",
            shape(normalize_node_type(&n.node_type), &label)
        ));
    }
    for e in edges {
        let from = safe_id(&e.source_id);
        let to = safe_id(&e.target_id);
        let label = sanitize_label(&e.relation_label);
        if label == "…" {
            out.push_str(&format!("  {from} --> {to}\n"));
        } else {
            out.push_str(&format!("  {from} -->|\"{label}\"| {to}\n"));
        }
    }
    out.push_str(CLASS_DEFS);
    out
}

/// Clean up a Mermaid diagram produced by an LLM: strip fences, make sure it has a header
/// and the Triad classDefs. Returns `None` if nothing diagram-like survived.
pub fn sanitize_llm_mermaid(raw: &str) -> Option<String> {
    let mut text = raw.trim().to_string();
    static FENCE: OnceLock<Regex> = OnceLock::new();
    if let Some(c) = re(&FENCE, r"(?s)```(?:mermaid)?\s*(.*?)```").captures(&text) {
        text = c[1].trim().to_string();
    }
    static HEAD: OnceLock<Regex> = OnceLock::new();
    let head = re(&HEAD, r"(?m)^\s*(graph|flowchart)\s+(TD|TB|LR|RL|BT)\b");
    let start = head.find(&text)?.start();
    let mut body: Vec<String> = text[start..]
        .lines()
        .map(|l| l.trim_end().to_string())
        .filter(|l| !l.trim_start().starts_with("%%"))
        .collect();
    if body.len() > 160 {
        body.truncate(160);
    }
    let mut text = body.join("\n");
    if !text.contains("classDef physicist") {
        text.push('\n');
        text.push_str(CLASS_DEFS);
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_safe() {
        assert_eq!(safe_id("Энтропия (архитектуры)"), "entropiya_arhitektury");
        assert_eq!(safe_id("42 things"), "n_42_things");
        assert_eq!(safe_id("end"), "end_n");
    }

    #[test]
    fn labels_are_clean() {
        assert_eq!(sanitize_label("Обломов [на диване] (спит)"), "Обломов на диване спит");
    }
}
