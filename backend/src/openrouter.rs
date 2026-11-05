//! Async OpenRouter client (OpenAI-compatible `/chat/completions`) with assistant-prefill
//! support and a resilient JSON extractor. Falls back to the built-in mock LLM when no key.

use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicI64, AtomicU64, Ordering},
        Arc, OnceLock,
    },
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
    /// OpenAI-style repetition controls (ignored by providers that lack them).
    pub presence_penalty: Option<f32>,
    pub frequency_penalty: Option<f32>,
}

pub struct LlmClient {
    http: reqwest::Client,
    log: std::sync::Mutex<std::collections::VecDeque<CallLog>>,
    settings: Arc<RwLock<LlmSettings>>,
    app_url: String,
    /// Calls made today (UTC day in `day`), for the daily budget and the health endpoint.
    calls_today: AtomicU64,
    day: AtomicI64,
    pub daily_budget: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Usage {
    pub calls_today: u64,
    pub daily_budget: u64,
}

/// One LLM call as the UI's journal sees it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CallLog {
    pub at: String,
    pub kind: String,
    pub model: String,
    pub ms: u64,
    pub ok: bool,
    pub error: Option<String>,
    pub prompt_chars: usize,
    pub completion_chars: usize,
}

const LOG_CAP: usize = 200;

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
    pub fn new(settings: Arc<RwLock<LlmSettings>>, app_url: String, daily_budget: u64) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .context("cannot build http client")?;
        Ok(Self { http, settings, app_url, calls_today: AtomicU64::new(0), day: AtomicI64::new(today()), daily_budget, log: std::sync::Mutex::new(std::collections::VecDeque::new()) })
    }

    fn record(&self, kind: &str, model: &str, started: std::time::Instant, prompt_chars: usize, result: &Result<String>) {
        let entry = CallLog {
            at: chrono::Utc::now().to_rfc3339(),
            kind: kind.into(),
            model: model.into(),
            ms: started.elapsed().as_millis() as u64,
            ok: result.is_ok(),
            error: result.as_ref().err().map(|e| crate::mermaid::truncate_chars(&e.to_string(), 300)),
            prompt_chars,
            completion_chars: result.as_ref().map(|t| t.chars().count()).unwrap_or(0),
        };
        if let Ok(mut l) = self.log.lock() {
            l.push_back(entry);
            while l.len() > LOG_CAP {
                l.pop_front();
            }
        }
    }

    /// Newest-first journal of recent calls.
    pub fn journal(&self) -> Vec<CallLog> {
        self.log.lock().map(|l| l.iter().rev().cloned().collect()).unwrap_or_default()
    }

    pub fn usage(&self) -> Usage {
        self.roll_day();
        Usage { calls_today: self.calls_today.load(Ordering::Relaxed), daily_budget: self.daily_budget }
    }

    pub fn budget_left(&self) -> bool {
        self.roll_day();
        self.daily_budget == 0 || self.calls_today.load(Ordering::Relaxed) < self.daily_budget
    }

    fn roll_day(&self) {
        let t = today();
        if self.day.swap(t, Ordering::Relaxed) != t {
            self.calls_today.store(0, Ordering::Relaxed);
        }
    }

    /// Run a chat completion. The returned text already has the prefill prepended, so callers
    /// can split `<ego_thought>…</ego_thought>` deterministically.
    pub async fn chat(&self, model: &str, messages: &[ChatMessage], opts: &ChatOpts) -> Result<String> {
        let key = match self.settings.read().await.api_key.clone() {
            Some(k) => k,
            None => return Ok(mock_llm::respond(messages, opts)),
        };
        if !self.budget_left() {
            bail!("daily LLM budget exhausted ({} calls) — повысь DAILY_CALL_BUDGET или подожди до завтра", self.daily_budget);
        }
        let mut msgs = messages.to_vec();
        if let Some(p) = &opts.prefill {
            msgs.push(ChatMessage::assistant(p.clone()));
        }
        self.roll_day();
        self.calls_today.fetch_add(1, Ordering::Relaxed);
        let started = std::time::Instant::now();
        let prompt_chars: usize = msgs.iter().map(|m| m.content.chars().count()).sum();
        let result: Result<String> = async {
            match self.call_with_retry(&key, model, &msgs, opts, opts.json_mode).await {
                Ok(t) => Ok(t),
                Err(e) if opts.json_mode && e.to_string().contains("openrouter 4") && !e.to_string().contains("openrouter 429") => {
                    tracing::warn!(error = %e, "json_mode request rejected → retrying without response_format");
                    self.call_with_retry(&key, model, &msgs, opts, false).await
                }
                Err(e) => Err(e),
            }
        }
        .await;
        self.record(if opts.json_mode { "json" } else { "chat" }, model, started, prompt_chars, &result);
        let text = result?;
        Ok(match &opts.prefill {
            Some(p) if !text.trim_start().starts_with(p.as_str()) => format!("{p}{text}"),
            _ => text,
        })
    }

    /// Retries on 429 / 5xx / transport errors with backoff (free-tier models rate-limit hard).
    async fn call_with_retry(&self, key: &str, model: &str, msgs: &[ChatMessage], opts: &ChatOpts, json_mode: bool) -> Result<String> {
        let mut last = None;
        for (i, wait) in [0u64, 3, 8, 15].iter().enumerate() {
            if *wait > 0 {
                tokio::time::sleep(Duration::from_secs(*wait)).await;
            }
            match self.call(key, model, msgs, opts, json_mode).await {
                Ok(t) => return Ok(t),
                Err(e) => {
                    let msg = e.to_string();
                    let retryable = msg.contains("openrouter 429") || msg.contains("openrouter 5") || msg.contains("request failed") || msg.contains("timed out") || msg.contains("empty choices");
                    if !retryable || i == 3 {
                        return Err(e);
                    }
                    tracing::warn!(attempt = i + 1, error = %msg, "LLM call failed → retry");
                    last = Some(e);
                }
            }
        }
        Err(last.unwrap_or_else(|| anyhow!("LLM call failed")))
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
        if let Some(p) = opts.presence_penalty {
            body["presence_penalty"] = json!(p);
        }
        if let Some(p) = opts.frequency_penalty {
            body["frequency_penalty"] = json!(p);
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
        let text = content_to_text(content);
        if text.trim().is_empty() {
            bail!("openrouter: empty choices (null content)");
        }
        Ok(text)
    }
}

impl LlmClient {
    /// Streaming completion: `on_delta` receives text chunks as they arrive; the full text is
    /// returned at the end. Falls back to a plain call when the provider refuses to stream.
    pub async fn chat_stream<F: FnMut(&str)>(&self, model: &str, messages: &[ChatMessage], opts: &ChatOpts, mut on_delta: F) -> Result<String> {
        let key = match self.settings.read().await.api_key.clone() {
            Some(k) => k,
            None => {
                // Mock: emit the reply in word-sized chunks so the UI path gets exercised.
                let full = mock_llm::respond(messages, opts);
                for w in full.split_inclusive(' ') {
                    on_delta(w);
                    tokio::time::sleep(Duration::from_millis(12)).await;
                }
                return Ok(full);
            }
        };
        if !self.budget_left() {
            bail!("daily LLM budget exhausted ({} calls)", self.daily_budget);
        }
        let mut msgs = messages.to_vec();
        if let Some(p) = &opts.prefill {
            msgs.push(ChatMessage::assistant(p.clone()));
        }
        self.roll_day();
        self.calls_today.fetch_add(1, Ordering::Relaxed);
        let started = std::time::Instant::now();
        let prompt_chars: usize = msgs.iter().map(|m| m.content.chars().count()).sum();
        let mut body = json!({
            "model": model,
            "messages": msgs,
            "temperature": opts.temperature,
            "max_tokens": opts.max_tokens,
            "stream": true,
        });
        if let Some(p) = opts.presence_penalty {
            body["presence_penalty"] = json!(p);
        }
        if let Some(p) = opts.frequency_penalty {
            body["frequency_penalty"] = json!(p);
        }
        let resp = self
            .http
            .post("https://openrouter.ai/api/v1/chat/completions")
            .bearer_auth(&key)
            .header("HTTP-Referer", &self.app_url)
            .header("X-Title", "Triad Canvas")
            .json(&body)
            .send()
            .await
            .context("openrouter stream request failed")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let raw = resp.text().await.unwrap_or_default();
            if status.as_u16() == 429 || status.is_server_error() {
                tracing::warn!(%status, "stream refused → plain call with retries");
                return self.chat(model, messages, opts).await;
            }
            bail!("openrouter {status}: {}", crate::mermaid::truncate_chars(&raw, 300));
        }
        use futures::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buf = String::new();
        let mut full = String::new();
        let mut done = false;
        let mut finish: Option<String> = None;
        let mut broke: Option<String> = None;
        'outer: while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    broke = Some(e.to_string());
                    break;
                }
            };
            buf.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(pos) = buf.find('\n') {
                let line = buf[..pos].trim().to_string();
                buf.drain(..=pos);
                let Some(data) = line.strip_prefix("data:") else { continue };
                let data = data.trim();
                if data == "[DONE]" {
                    done = true;
                    break 'outer;
                }
                let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
                if let Some(err) = v.get("error") {
                    broke = Some(err.to_string());
                    break 'outer;
                }
                if let Some(f) = v.pointer("/choices/0/finish_reason").and_then(|f| f.as_str()) {
                    finish = Some(f.to_string());
                }
                if let Some(delta) = v.pointer("/choices/0/delta/content").and_then(|c| c.as_str()) {
                    if !delta.is_empty() {
                        full.push_str(delta);
                        on_delta(delta);
                    }
                }
            }
        }
        // A stream that broke, never finished, or finished "stop" after a few characters is a
        // dropped connection, not a line: get the line again without streaming (with retries).
        let suspicious = broke.is_some() || (!done && finish.is_none()) || (full.trim().chars().count() < 12 && finish.as_deref() != Some("length"));
        self.record("stream", model, started, prompt_chars, &if suspicious { Err(anyhow!("stream dropped after {} chars ({})", full.chars().count(), broke.clone().unwrap_or_else(|| "no [DONE]".into()))) } else { Ok(full.clone()) });
        if suspicious {
            tracing::warn!(chars = full.chars().count(), done, finish = ?finish, error = ?broke, "stream dropped → plain call");
            let text = self.chat(model, messages, opts).await?;
            // the UI saw a partial stream: send the remainder so the bubble ends on the real text
            if text.starts_with(full.trim_end()) && !full.is_empty() {
                on_delta(&text[full.trim_end().len()..]);
            } else {
                on_delta("\n");
                on_delta(&text);
            }
            return Ok(text);
        }
        if full.trim().is_empty() {
            bail!("empty stream");
        }
        Ok(match &opts.prefill {
            Some(p) if !full.trim_start().starts_with(p.as_str()) => format!("{p}{full}"),
            _ => full,
        })
    }
}

/// A tool call requested by the model.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub args: Value,
}

/// Result of one tools-enabled round: either the model asked for tools, or it answered.
pub enum ToolTurn {
    Calls { assistant_message: Value, calls: Vec<ToolCall> },
    Text(String),
}

impl LlmClient {
    /// One step of a native tool-calling conversation (OpenAI format). `history` holds the
    /// running message list including prior assistant tool_calls and `tool` results.
    pub async fn chat_tools_step(&self, model: &str, history: &[Value], tools: &[Value], opts: &ChatOpts) -> Result<ToolTurn> {
        let key = self.settings.read().await.api_key.clone().ok_or_else(|| anyhow!("no api key"))?;
        if !self.budget_left() {
            bail!("daily LLM budget exhausted ({} calls)", self.daily_budget);
        }
        self.roll_day();
        self.calls_today.fetch_add(1, Ordering::Relaxed);
        let mut body = json!({
            "model": model,
            "messages": history,
            "temperature": opts.temperature,
            "max_tokens": opts.max_tokens,
            "tools": tools,
            "tool_choice": "auto",
        });
        if let Some(p) = opts.presence_penalty {
            body["presence_penalty"] = json!(p);
        }
        if let Some(p) = opts.frequency_penalty {
            body["frequency_penalty"] = json!(p);
        }
        let started = std::time::Instant::now();
        let prompt_chars: usize = history.iter().filter_map(|m| m.get("content").and_then(|c| c.as_str())).map(|c| c.chars().count()).sum();
        let r = self.tools_step_inner(&key, &body).await;
        let summary: Result<String> = match &r {
            Ok(ToolTurn::Text(t)) => Ok(t.clone()),
            Ok(ToolTurn::Calls { calls, .. }) => Ok(format!("tools: {}", calls.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(","))),
            Err(e) => Err(anyhow!("{e}")),
        };
        self.record("tools", model, started, prompt_chars, &summary);
        r
    }

    async fn tools_step_inner(&self, key: &str, body: &Value) -> Result<ToolTurn> {
        let mut last_err = None;
        for (i, wait) in [0u64, 3, 8].iter().enumerate() {
            if *wait > 0 {
                tokio::time::sleep(Duration::from_secs(*wait)).await;
            }
            let resp = self
                .http
                .post("https://openrouter.ai/api/v1/chat/completions")
                .bearer_auth(key)
                .header("HTTP-Referer", &self.app_url)
                .header("X-Title", "Triad Canvas")
                .json(body)
                .send()
                .await;
            let resp = match resp {
                Ok(r) => r,
                Err(e) => {
                    last_err = Some(anyhow!("openrouter request failed: {e}"));
                    continue;
                }
            };
            let status = resp.status();
            let raw = resp.text().await.unwrap_or_default();
            if status.as_u16() == 429 || status.is_server_error() {
                last_err = Some(anyhow!("openrouter {status}"));
                if i < 2 {
                    continue;
                }
                bail!("openrouter {status}: {}", crate::mermaid::truncate_chars(&raw, 300));
            }
            if !status.is_success() {
                bail!("openrouter {status}: {}", crate::mermaid::truncate_chars(&raw, 400));
            }
            let v: Value = serde_json::from_str(&raw).context("openrouter non-JSON")?;
            let msg = v.pointer("/choices/0/message").cloned().ok_or_else(|| anyhow!("empty choices"))?;
            if let Some(calls) = msg.get("tool_calls").and_then(|c| c.as_array()).filter(|c| !c.is_empty()) {
                let parsed = calls
                    .iter()
                    .filter_map(|c| {
                        let id = c.get("id").and_then(|x| x.as_str()).unwrap_or("call").to_string();
                        let name = c.pointer("/function/name")?.as_str()?.to_string();
                        let args_raw = c.pointer("/function/arguments").and_then(|a| a.as_str()).unwrap_or("{}");
                        let args = serde_json::from_str::<Value>(args_raw).or_else(|_| extract_json(args_raw)).unwrap_or(json!({}));
                        Some(ToolCall { id, name, args })
                    })
                    .collect();
                return Ok(ToolTurn::Calls { assistant_message: msg, calls: parsed });
            }
            let text = msg.get("content").cloned().map(content_to_text).unwrap_or_default();
            return Ok(ToolTurn::Text(text));
        }
        Err(last_err.unwrap_or_else(|| anyhow!("tool step failed")))
    }
}

impl LlmClient {
    /// Vision: describe an image for the inhabitants (data URL inline). Returns plain text.
    pub async fn describe_image(&self, model: &str, bytes: &[u8], mime: &str, prompt: &str) -> Result<String> {
        let key = self.settings.read().await.api_key.clone().ok_or_else(|| anyhow!("no api key"))?;
        if !self.budget_left() {
            bail!("daily LLM budget exhausted");
        }
        use base64::Engine;
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        let body = json!({
            "model": model,
            "max_tokens": 500,
            "temperature": 0.3,
            "messages": [{ "role": "user", "content": [
                { "type": "text", "text": prompt },
                { "type": "image_url", "image_url": { "url": format!("data:{mime};base64,{data}") } }
            ]}]
        });
        self.roll_day();
        self.calls_today.fetch_add(1, Ordering::Relaxed);
        let started = std::time::Instant::now();
        let result: Result<String> = async {
            let resp = self.http.post("https://openrouter.ai/api/v1/chat/completions").bearer_auth(&key).header("HTTP-Referer", &self.app_url).header("X-Title", "Triad Canvas").json(&body).send().await.context("openrouter request failed")?;
            let status = resp.status();
            let raw = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                bail!("openrouter {status}: {}", crate::mermaid::truncate_chars(&raw, 300));
            }
            let v: Value = serde_json::from_str(&raw).context("non-JSON")?;
            let text = v.pointer("/choices/0/message/content").cloned().map(content_to_text).unwrap_or_default();
            if text.trim().is_empty() {
                bail!("empty vision reply");
            }
            Ok(text)
        }
        .await;
        self.record("vision", model, started, prompt.chars().count(), &result);
        result
    }
}

fn today() -> i64 {
    chrono::Utc::now().format("%Y%m%d").to_string().parse().unwrap_or(0)
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
        Value::Null => String::new(),
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
