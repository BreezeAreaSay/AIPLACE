//! Voices: text-to-speech and speech-to-text through OpenAI-compatible audio endpoints
//! (OpenAI, Groq, local servers). Without keys the UI falls back to the browser's own voices.

use anyhow::{Context, Result};

use super::http;
use crate::config::LlmSettings;

const DEFAULT_TTS: &str = "https://api.openai.com/v1/audio/speech";
const DEFAULT_STT: &str = "https://api.groq.com/openai/v1/audio/transcriptions";

/// `physicist=onyx,lit_teacher=nova,graffiti=echo` → voice for an agent (default alloy).
pub fn voice_for(s: &LlmSettings, agent_id: &str, override_voice: &str) -> String {
    if !override_voice.trim().is_empty() {
        return override_voice.trim().to_string();
    }
    s.get("TTS_VOICES")
        .and_then(|m| m.split([',', ';']).filter_map(|kv| kv.split_once('=')).find(|(k, _)| k.trim() == agent_id).map(|(_, v)| v.trim().to_string()))
        .unwrap_or_else(|| match agent_id {
            "physicist" => "onyx".into(),
            "lit_teacher" => "nova".into(),
            "graffiti" => "echo".into(),
            _ => "alloy".into(),
        })
}

pub async fn tts(s: &LlmSettings, text: &str, agent_id: &str, override_voice: &str) -> Result<(Vec<u8>, String)> {
    anyhow::ensure!(s.tts_enabled(), "TTS не настроен");
    let url = s.get("TTS_URL").unwrap_or(DEFAULT_TTS);
    let model = s.get("TTS_MODEL").unwrap_or("tts-1");
    let mut req = http().post(url).json(&serde_json::json!({ "model": model, "input": crate::mermaid::truncate_chars(text, 2000), "voice": voice_for(s, agent_id, override_voice), "response_format": "mp3" }));
    if let Some(k) = s.get("TTS_API_KEY") {
        req = req.bearer_auth(k);
    }
    let res = req.send().await?;
    if !res.status().is_success() {
        let st = res.status();
        let body = res.text().await.unwrap_or_default();
        anyhow::bail!("TTS {st}: {}", crate::mermaid::truncate_chars(&body, 200));
    }
    let ct = res.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("audio/mpeg").to_string();
    Ok((res.bytes().await?.to_vec(), ct))
}

pub async fn stt(s: &LlmSettings, bytes: Vec<u8>, filename: &str, mime: &str) -> Result<String> {
    anyhow::ensure!(s.stt_enabled(), "STT не настроен");
    let url = s.get("STT_URL").unwrap_or(DEFAULT_STT);
    let model = s.get("STT_MODEL").unwrap_or("whisper-large-v3");
    let part = reqwest::multipart::Part::bytes(bytes).file_name(filename.to_string()).mime_str(mime).unwrap_or_else(|_| reqwest::multipart::Part::bytes(vec![]));
    let form = reqwest::multipart::Form::new().text("model", model.to_string()).text("language", "ru").text("response_format", "json").part("file", part);
    let mut req = http().post(url).multipart(form);
    if let Some(k) = s.get("STT_API_KEY") {
        req = req.bearer_auth(k);
    }
    let res = req.send().await?;
    if !res.status().is_success() {
        let st = res.status();
        let body = res.text().await.unwrap_or_default();
        anyhow::bail!("STT {st}: {}", crate::mermaid::truncate_chars(&body, 200));
    }
    let v: serde_json::Value = res.json().await?;
    Ok(v["text"].as_str().context("нет text в ответе STT")?.trim().to_string())
}
