//! Async OpenRouter client (OpenAI-compatible `/chat/completions`) with assistant-prefill
//! support and a resilient JSON extractor. Falls back to the built-in mock LLM when no key.

use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::sync::RwLock;

use crate::{config::LlmSettings, mock_llm};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(c: impl Into<String>) -> Self {
        Self { role: "system".into(), content: c.into() }
    }
    pub fn user(c: impl Into<String>) -> Self {
        Self { role: "user".into(), content: c.into() }
    }
    pub fn assistant(c: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: c.into() }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ChatOpts {
    pub temperature: f32,
    pub max_tokens: u32,
    /// Ask for `response_format: json_object`; silently retried without it if the model refuses.
    pub json_mode: bool,
    /// Trailing `assistant` message — the model continues from here (`<ego_thought>`).
    pub prefill: Option<String>,
}

pub struct LlmClient {
    http: reqwest::Client,
    settings: Arc<RwLock<LlmSettings>>,
    app_url: String,
}

#[derive(Deserialize)]
struct Completion {
    choices: Option<Vec<Choice>>,
    error: Option<Value>,
}
#[derive(Deserialize)]
struct Choice {
    message: Option<ChoiceMessage>,
}
#[derive(Deserialize)]
struct ChoiceMessage {
    content: Option<Value>,
}

impl LlmClient {
    pub fn new(settings: Arc<RwLock<LlmSettings>>, app_url: String) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .context("cannot build http client")?;
        Ok(Self { http, settings, app_url })
    }

    /// Run a chat completion. The returned text already has the prefill prepended, so callers
    /// can split `<ego_thought>…</ego_thought>` deterministically.
    pub async fn chat(&self, model: &str, messages: &[ChatMessage], opts: &ChatOpts) -> Result<String> {
        let key = match self.settings.read().await.api_key.clone() {
            Some(k) => k,
            None => return Ok(mock_llm::respond(messages, opts)),
        };
        let mut msgs = messages.to_vec();
        if let Some(p) = &opts.prefill {
            msgs.push(ChatMessage::assistant(p.clone()));
        }
        let text = match self.call(&key, model, &msgs, opts, opts.json_mode).await {
            Ok(t) => t,
            Err(e) if opts.json_mode => {
                tracing::warn!(error = %e, "json_mode request failed → retrying without response_format");
                self.call(&key, model, &msgs, opts, false).await?
            }
            Err(e) => return Err(e),
        };
        Ok(match &opts.prefill {
            Some(p) if !text.trim_start().starts_with(p.as_str()) => format!("{p}{text}"),
            _ => text,
        })
    }

    async fn call(&self, key: &str, model: &str, msgs: &[ChatMessage], opts: &ChatOpts, json_mode: bool) -> Result<String> {
        let mut body = json!({
            "model": model,
            "messages": msgs,
            "temperature": opts.temperature,
            "max_tokens": opts.max_tokens,
        });
        if json_mode {
            body["response_format"] = json!({ "type": "json_object" });
        }
        let resp = self
            .http
            .post("https://openrouter.ai/api/v1/chat/completions")
            .bearer_auth(key)
            .header("HTTP-Referer", &self.app_url)
            .header("X-Title", "Triad Canvas")
            .json(&body)
            .send()
            .await
            .context("openrouter request failed")?;
        let status = resp.status();
        let raw = resp.text().await.context("openrouter body unreadable")?;
        if !status.is_success() {
            bail!("openrouter {status}: {}", crate::mermaid::truncate_chars(&raw, 400));
        }
        let parsed: Completion =
            serde_json::from_str(&raw).with_context(|| format!("openrouter returned non-JSON: {}", crate::mermaid::truncate_chars(&raw, 300)))?;
        if let Some(err) = parsed.error {
            bail!("openrouter error: {err}");
        }
        let content = parsed
            .choices
            .and_then(|c| c.into_iter().next())
            .and_then(|c| c.message)
            .and_then(|m| m.content)
            .ok_or_else(|| anyhow!("openrouter: empty choices"))?;
        Ok(content_to_text(content))
    }
}

/// Some providers return content as an array of parts instead of a string.
fn content_to_text(v: Value) -> String {
    match v {
        Value::String(s) => s,
        Value::Array(parts) => parts
            .into_iter()
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()).map(str::to_string))
            .collect::<Vec<_>>()
            .join(""),
        other => other.to_string(),
    }
}

/// Pull a JSON object out of whatever the model produced: raw JSON, ```json fences,
/// chatter around it, or trailing commas.
pub fn extract_json(raw: &str) -> Result<Value> {
    let trimmed = raw.trim();
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return Ok(v);
    }
    static FENCE: OnceLock<Regex> = OnceLock::new();
    let fence = FENCE.get_or_init(|| Regex::new(r"(?s)```(?:json)?\s*(\{.*?\})\s*```").expect("regex"));
    if let Some(c) = fence.captures(trimmed) {
        if let Ok(v) = serde_json::from_str::<Value>(&c[1]) {
            return Ok(v);
        }
    }
    let start = trimmed.find('{');
    let end = trimmed.rfind('}');
    if let (Some(s), Some(e)) = (start, end) {
        if e > s {
            let slice = &trimmed[s..=e];
            if let Ok(v) = serde_json::from_str::<Value>(slice) {
                return Ok(v);
            }
            static TRAILING: OnceLock<Regex> = OnceLock::new();
            let fixed = TRAILING
                .get_or_init(|| Regex::new(r",\s*([}\]])").expect("regex"))
                .replace_all(slice, "$1");
            if let Ok(v) = serde_json::from_str::<Value>(&fixed) {
                return Ok(v);
            }
        }
    }
    bail!("no parseable JSON object in LLM output: {}", crate::mermaid::truncate_chars(trimmed, 200))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_fenced_and_dirty_json() {
        let v = extract_json("Вот план:\n```json\n{\"a\": 1,}\n```\nготово").unwrap();
        assert_eq!(v["a"], 1);
        let v = extract_json("blah {\"x\": [1,2,],} tail").unwrap();
        assert_eq!(v["x"][1], 2);
    }
}
