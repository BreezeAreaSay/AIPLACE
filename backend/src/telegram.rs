//! Telegram Mini Apps: `initData` validation (HMAC-SHA256, per the official scheme) and a
//! tiny long-polling bot that answers `/start` with an "open the room" button.

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use anyhow::{anyhow, bail, Context, Result};
use axum::http::HeaderMap;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Sha256;
use tokio::sync::RwLock;

use crate::config::Config;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TgUser {
    pub id: i64,
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub last_name: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub language_code: Option<String>,
}

/// `Authorization: tma <initData>` (recommended by Telegram) or `?tma=<initData>` (for SSE,
/// where EventSource cannot set headers).
pub fn extract_init_data(headers: &HeaderMap, query: Option<&str>) -> Option<String> {
    if let Some(v) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        let v = v.trim();
        if let Some(rest) = v.strip_prefix("tma ").or_else(|| v.strip_prefix("TMA ")) {
            let rest = rest.trim();
            if !rest.is_empty() {
                return Some(rest.to_string());
            }
        }
    }
    if let Some(v) = headers.get("x-telegram-init-data").and_then(|v| v.to_str().ok()) {
        if !v.trim().is_empty() {
            return Some(v.trim().to_string());
        }
    }
    let q = query?;
    form_urlencoded::parse(q.as_bytes())
        .find(|(k, _)| k == "tma")
        .map(|(_, v)| v.into_owned())
        .filter(|v| !v.is_empty())
}

/// Verify the signature and freshness of `initData`; return the user.
pub fn validate_init_data(init_data: &str, bot_token: &str, max_age_secs: i64) -> Result<TgUser> {
    let pairs: Vec<(String, String)> = form_urlencoded::parse(init_data.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let hash = pairs
        .iter()
        .find(|(k, _)| k == "hash")
        .map(|(_, v)| v.clone())
        .ok_or_else(|| anyhow!("initData: no hash"))?;
    let mut sorted: BTreeMap<&str, &str> = BTreeMap::new();
    for (k, v) in &pairs {
        if k != "hash" {
            sorted.insert(k.as_str(), v.as_str());
        }
    }
    let data_check: String = sorted.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("\n");

    let mut secret = HmacSha256::new_from_slice(b"WebAppData").context("hmac")?;
    secret.update(bot_token.as_bytes());
    let secret_key = secret.finalize().into_bytes();
    let mut mac = HmacSha256::new_from_slice(&secret_key).context("hmac")?;
    mac.update(data_check.as_bytes());
    let expected = hex::encode(mac.finalize().into_bytes());
    if !constant_time_eq(expected.as_bytes(), hash.as_bytes()) {
        bail!("initData: bad signature");
    }
    let auth_date: i64 = sorted
        .get("auth_date")
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| anyhow!("initData: no auth_date"))?;
    let now = chrono::Utc::now().timestamp();
    if max_age_secs > 0 && now - auth_date > max_age_secs {
        bail!("initData: expired");
    }
    let user_json = sorted.get("user").ok_or_else(|| anyhow!("initData: no user"))?;
    let user: TgUser = serde_json::from_str(user_json).context("initData: user json")?;
    Ok(user)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub fn room_id_for(user: &TgUser) -> String {
    format!("tg_{}", user.id)
}

// ───────────────────────────── /start bot (long polling) ─────────────────────────────

#[derive(Deserialize)]
struct TgResponse<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
}
#[derive(Deserialize)]
struct Me {
    username: Option<String>,
}
#[derive(Deserialize)]
struct Update {
    update_id: i64,
    message: Option<Msg>,
}
#[derive(Deserialize)]
struct Msg {
    chat: Chat,
    text: Option<String>,
    from: Option<TgUser>,
}
#[derive(Deserialize)]
struct Chat {
    id: i64,
}

pub async fn get_me(client: &reqwest::Client, token: &str) -> Result<Option<String>> {
    let r: TgResponse<Me> = client
        .get(format!("https://api.telegram.org/bot{token}/getMe"))
        .send()
        .await?
        .json()
        .await?;
    if !r.ok {
        bail!("getMe failed: {}", r.description.unwrap_or_default());
    }
    Ok(r.result.and_then(|m| m.username))
}

/// Runs forever; errors are logged and retried. Answers `/start` with a web_app button.
pub async fn run_bot(cfg: Arc<Config>, username_slot: Arc<RwLock<Option<String>>>) {
    let Some(token) = cfg.telegram_bot_token.clone() else { return };
    let client = match reqwest::Client::builder().timeout(Duration::from_secs(70)).build() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "telegram client");
            return;
        }
    };
    match get_me(&client, &token).await {
        Ok(name) => {
            tracing::info!(bot = ?name, webapp = %cfg.webapp_url, "Telegram bot online (long polling)");
            *username_slot.write().await = name;
        }
        Err(e) => tracing::warn!(error = %e, "Telegram getMe failed — check TELEGRAM_BOT_TOKEN"),
    }
    if !cfg.webapp_url.starts_with("https://") {
        tracing::warn!(url = %cfg.webapp_url, "WEBAPP_URL is not https — Telegram will refuse to open the Mini App button");
    }
    let mut offset: i64 = 0;
    loop {
        let res = client
            .get(format!("https://api.telegram.org/bot{token}/getUpdates"))
            .query(&[("timeout", "50"), ("offset", &offset.to_string()), ("allowed_updates", "[\"message\"]")])
            .send()
            .await;
        let updates: Vec<Update> = match res {
            Ok(r) => match r.json::<TgResponse<Vec<Update>>>().await {
                Ok(j) if j.ok => j.result.unwrap_or_default(),
                Ok(j) => {
                    tracing::warn!(desc = ?j.description, "getUpdates not ok");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "getUpdates parse");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            },
            Err(e) => {
                tracing::warn!(error = %e, "getUpdates request");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };
        for u in updates {
            offset = offset.max(u.update_id + 1);
            let Some(m) = u.message else { continue };
            let text = m.text.unwrap_or_default();
            if text.starts_with("/start") || text.starts_with("/room") || text.starts_with("/help") {
                let name = m.from.map(|f| f.first_name).unwrap_or_else(|| "гость".into());
                let body = json!({
                    "chat_id": m.chat.id,
                    "text": format!("{name}, комната ждёт. Трое внутри уже спорят о тебе.\n\nФизик, литератор и граффитист — с HP, дофамином, скрытыми целями и памятью. Дай им слово — и держи его."),
                    "reply_markup": {"inline_keyboard": [[{"text": "⚡ Открыть комнату", "web_app": {"url": cfg.webapp_url}}]]}
                });
                if let Err(e) = client
                    .post(format!("https://api.telegram.org/bot{token}/sendMessage"))
                    .json(&body)
                    .send()
                    .await
                {
                    tracing::warn!(error = %e, "sendMessage");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign(token: &str, fields: &[(&str, &str)]) -> String {
        let mut sorted: BTreeMap<&str, &str> = fields.iter().cloned().collect();
        sorted.remove("hash");
        let data_check: String = sorted.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("\n");
        let mut secret = HmacSha256::new_from_slice(b"WebAppData").unwrap();
        secret.update(token.as_bytes());
        let mut mac = HmacSha256::new_from_slice(&secret.finalize().into_bytes()).unwrap();
        mac.update(data_check.as_bytes());
        let hash = hex::encode(mac.finalize().into_bytes());
        let mut ser = form_urlencoded::Serializer::new(String::new());
        for (k, v) in fields {
            ser.append_pair(k, v);
        }
        ser.append_pair("hash", &hash);
        ser.finish()
    }

    #[test]
    fn validates_signed_init_data() {
        let token = "123456:ABC-DEF";
        let now = chrono::Utc::now().timestamp().to_string();
        let user = r#"{"id":42,"first_name":"Иван","username":"ivan"}"#;
        let init = sign(token, &[("auth_date", &now), ("query_id", "AAE"), ("user", user)]);
        let u = validate_init_data(&init, token, 86400).unwrap();
        assert_eq!(u.id, 42);
        assert_eq!(u.first_name, "Иван");
        assert!(validate_init_data(&init, "other:token", 86400).is_err());
        let tampered = init.replace("ivan", "eve");
        assert!(validate_init_data(&tampered, token, 86400).is_err());
    }

    #[test]
    fn rejects_stale() {
        let token = "t";
        let init = sign(token, &[("auth_date", "1000"), ("user", r#"{"id":1}"#)]);
        assert!(validate_init_data(&init, token, 3600).is_err());
    }
}
