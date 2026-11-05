//! OpenRouter model catalog (public endpoint, works without a key), cached for an hour and
//! trimmed to what the settings UI needs: price, context, free tier, JSON support.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub context_length: u64,
    /// USD per 1M tokens.
    pub prompt_price: f64,
    pub completion_price: f64,
    pub free: bool,
    pub json_mode: bool,
    pub tools: bool,
}

#[derive(Default)]
pub struct Catalog {
    cache: RwLock<Option<(Instant, Vec<ModelInfo>)>>,
}

#[derive(Deserialize)]
struct Raw {
    data: Vec<RawModel>,
}
#[derive(Deserialize)]
struct RawModel {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    context_length: Option<u64>,
    #[serde(default)]
    pricing: Option<RawPricing>,
    #[serde(default)]
    supported_parameters: Option<Vec<String>>,
}
#[derive(Deserialize)]
struct RawPricing {
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    completion: Option<String>,
}

fn per_million(s: &Option<String>) -> f64 {
    s.as_deref().and_then(|v| v.parse::<f64>().ok()).map(|v| v * 1_000_000.0).unwrap_or(0.0)
}

impl Catalog {
    /// Does this model advertise native tool calling? Unknown models → false (tags fallback).
    pub async fn supports_tools(&self, api_key: Option<&str>, model: &str) -> bool {
        match self.models(api_key).await {
            Ok(list) => list.iter().find(|m| m.id == model).map(|m| m.tools).unwrap_or(false),
            Err(_) => false,
        }
    }

    pub async fn models(&self, api_key: Option<&str>) -> Result<Vec<ModelInfo>> {
        if let Some((at, list)) = self.cache.read().await.as_ref() {
            if at.elapsed() < Duration::from_secs(3600) {
                return Ok(list.clone());
            }
        }
        let client = reqwest::Client::builder().timeout(Duration::from_secs(20)).build()?;
        let mut req = client.get("https://openrouter.ai/api/v1/models");
        if let Some(k) = api_key {
            req = req.bearer_auth(k);
        }
        let raw: Raw = req.send().await.context("openrouter models")?.error_for_status()?.json().await?;
        let mut list: Vec<ModelInfo> = raw
            .data
            .into_iter()
            .map(|m| {
                let pp = m.pricing.as_ref().map(|p| per_million(&p.prompt)).unwrap_or(0.0);
                let cp = m.pricing.as_ref().map(|p| per_million(&p.completion)).unwrap_or(0.0);
                let free = m.id.ends_with(":free") || (pp == 0.0 && cp == 0.0);
                let json_mode = m
                    .supported_parameters
                    .as_ref()
                    .map(|p| p.iter().any(|x| x == "response_format" || x == "structured_outputs"))
                    .unwrap_or(false);
                let tools = m.supported_parameters.as_ref().map(|p| p.iter().any(|x| x == "tools" || x == "tool_choice")).unwrap_or(false);
                ModelInfo {
                    tools,
                    name: if m.name.is_empty() { m.id.clone() } else { m.name },
                    id: m.id,
                    context_length: m.context_length.unwrap_or(0),
                    prompt_price: pp,
                    completion_price: cp,
                    free,
                    json_mode,
                }
            })
            .collect();
        list.sort_by(|a, b| b.free.cmp(&a.free).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        *self.cache.write().await = Some((Instant::now(), list.clone()));
        Ok(list)
    }
}
