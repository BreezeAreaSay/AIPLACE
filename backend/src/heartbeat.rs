//! The room's own pulse. When the player has been silent for a while (and it is not the
//! quiet hours, and the daily budget allows), one small thing happens in every open room:
//! someone writes first, two of them argue, someone annotates an object, someone advances
//! their own project. Messages produced this way are what the player finds "while away".

use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Local, Timelike, Utc};
use rand::Rng;

use crate::{
    db,
    director::Trigger,
    models::RoomEvent,
    state::AppState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifeKind {
    /// One inhabitant addresses the player first (about their things or their own).
    WriteFirst,
    /// Two inhabitants argue among themselves.
    Argue,
    /// One inhabitant advances their own project and tells about it.
    OwnProject,
    /// Someone brings a fresh item from the RSS feeds.
    News,
}

impl LifeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LifeKind::WriteFirst => "write_first",
            LifeKind::Argue => "argue",
            LifeKind::OwnProject => "own_project",
            LifeKind::News => "news",
        }
    }
}

fn in_quiet_hours(spec: &str) -> bool {
    let Some((a, b)) = spec.split_once('-') else { return false };
    let (Ok(a), Ok(b)) = (a.trim().parse::<u32>(), b.trim().parse::<u32>()) else { return false };
    let h = Local::now().hour();
    if a == b {
        return false;
    }
    if a < b {
        h >= a && h < b
    } else {
        h >= a || h < b
    }
}

fn minutes_since(ts: &str) -> Option<i64> {
    let parsed = DateTime::parse_from_rfc3339(ts)
        .map(|d| d.with_timezone(&Utc))
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.fZ").map(|n| n.and_utc()))
        .ok()?;
    Some((Utc::now() - parsed).num_minutes())
}

pub async fn run(root: AppState) {
    tracing::info!(minutes = root.cfg.heartbeat_minutes, quiet = %root.cfg.quiet_hours, "heartbeat on");
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        if let Err(e) = beat_all(&root).await {
            tracing::warn!(error = %e, "heartbeat iteration failed");
        }
    }
}

async fn beat_all(root: &AppState) -> Result<()> {
    // Feeds are polled regardless of the player's presence or the LLM budget (no LLM involved).
    for id in root.rooms.open_ids().await {
        if let Ok(app) = root.for_room(&id).await {
            if let Err(e) = maybe_poll_feeds(&app).await {
                tracing::warn!(room = %id, error = %e, "feed poll failed");
            }
        }
    }
    // Evening chronicle and file digestion don't need the player to be away — only budget.
    if root.llm.budget_left() && (!root.llm_settings().await.mock_mode() || root.cfg.heartbeat_in_mock) {
        for id in root.rooms.open_ids().await {
            if let Ok(app) = root.for_room(&id).await {
                if let Err(e) = maybe_chronicle(&app).await {
                    tracing::warn!(room = %id, error = %e, "chronicle failed");
                }
                if let Err(e) = maybe_digest(&app).await {
                    tracing::warn!(room = %id, error = %e, "digest failed");
                }
            }
        }
    }
    if in_quiet_hours(&root.cfg.quiet_hours) {
        return Ok(());
    }
    if root.llm_settings().await.mock_mode() && !root.cfg.heartbeat_in_mock {
        return Ok(());
    }
    if !root.llm.budget_left() {
        return Ok(());
    }
    for id in root.rooms.open_ids().await {
        let app = root.for_room(&id).await?;
        if let Err(e) = maybe_beat(&app).await {
            tracing::warn!(room = %id, error = %e, "heartbeat failed");
        }
    }
    Ok(())
}

async fn maybe_beat(app: &AppState) -> Result<()> {
    let min = app.cfg.heartbeat_minutes as i64;
    // Silence since the player's last line AND since the room's last line (no spam while away).
    let since_user = db::last_user_activity(&app.db).await?.as_deref().and_then(minutes_since);
    let since_any = db::last_message_time(&app.db).await?.as_deref().and_then(minutes_since).unwrap_or(i64::MAX);
    let Some(since_user) = since_user else { return Ok(()) }; // never spoke → nothing to react to
    if since_user < min || since_any < min {
        return Ok(());
    }
    // Back off: the longer the player is away, the rarer the pulse (min, 2min, 4min… capped at a day).
    let beats_while_away: i64 = db::meta_get(&app.db, "beats_while_away").await?.and_then(|v| v.parse().ok()).unwrap_or(0);
    let required_gap = (min * (1 << beats_while_away.min(5))).min(24 * 60);
    if since_any < required_gap {
        return Ok(());
    }
    if app.turn_lock.try_lock().is_err() {
        return Ok(());
    }
    let alive = db::list_agents(&app.db).await?.into_iter().filter(|a| a.is_alive()).count();
    if alive == 0 {
        return Ok(());
    }
    let has_news = !crate::integrations::feeds::fresh_news(app, 1).await.unwrap_or_default().is_empty();
    let needs_care = !crate::organism::needs_care(app, 1).await.unwrap_or_default().is_empty();
    let roll = rand::thread_rng().gen_range(0..100);
    // Every third quiet beat (when there is something to tend) is care, not talk: one small call.
    if needs_care && roll < 33 {
        if app.turn_lock.try_lock().is_err() {
            return Ok(());
        }
        let _turn = app.turn_lock.lock().await;
        match crate::organism::tend_next(app).await {
            Ok(Some((agent, node, n))) => tracing::info!(room = %app.room_id, %agent, %node, actions = n, "heartbeat: tended the library"),
            Ok(None) => {}
            Err(e) => tracing::warn!(error = %e, "tend failed"),
        }
        drop(_turn);
        db::meta_set(&app.db, "beats_while_away", &(beats_while_away + 1).to_string()).await?;
        app.emit(RoomEvent::Done);
        return Ok(());
    }
    let kind = {
        match roll {
            0..=39 => LifeKind::WriteFirst,
            40..=64 => LifeKind::Argue,
            65..=84 if has_news => LifeKind::News,
            _ => LifeKind::OwnProject,
        }
    };
    tracing::info!(room = %app.room_id, kind = kind.as_str(), away_min = since_user, "heartbeat: the room acts");
    let before = db::last_message_id(&app.db).await?;
    let _turn = app.turn_lock.lock().await;
    crate::handlers::run_round(app, Trigger::Life(kind)).await?;
    if matches!(kind, LifeKind::OwnProject) {
        // what they told the room goes into their project's log too
        if let Ok(new) = db::messages_between(&app.db, before, i64::MAX, 3).await {
            if let Some(m) = new.iter().rev().find(|m| m.sender_id != "system" && m.sender_id != "user") {
                let _ = crate::organism::advance_project(app, &m.sender_id, &m.content).await;
            }
        }
    }
    drop(_turn);
    if matches!(kind, LifeKind::News) {
        // the freshest items were offered to the Caster; don't offer them twice
        for n in crate::integrations::feeds::fresh_news(app, 6).await.unwrap_or_default() {
            let mut p = n.props.0.clone();
            p["discussed"] = serde_json::Value::Bool(true);
            let _ = db::update_object_meta(&app.db, &n.id, None, Some(&p), None).await;
        }
    }
    db::meta_set(&app.db, "beats_while_away", &(beats_while_away + 1).to_string()).await?;
    let after = db::last_message_id(&app.db).await?;
    if after > before {
        notify_telegram(app, before).await;
    }
    app.emit(RoomEvent::Done);
    Ok(())
}

/// Telegram rooms get a push with the first new line and a button to open the Mini App.
async fn notify_telegram(app: &AppState, after_id: i64) {
    let Some(token) = app.cfg.telegram_bot_token.clone() else { return };
    let Some(chat_id) = app.room_id.strip_prefix("tg_").and_then(|s| s.parse::<i64>().ok()) else { return };
    let Ok(new) = db::messages_between(&app.db, after_id, i64::MAX, 5).await else { return };
    let Ok(agents) = db::list_agents(&app.db).await else { return };
    let Some(first) = new.iter().find(|m| m.sender_id != "system" && m.sender_id != "user") else { return };
    let who = crate::prompts::sender_name(&first.sender_id, &agents);
    let text = format!("{who}: {}", crate::mermaid::truncate_chars(&first.content, 300));
    let body = serde_json::json!({
        "chat_id": chat_id,
        "text": text,
        "reply_markup": {"inline_keyboard": [[{"text": "Открыть комнату", "web_app": {"url": app.cfg.webapp_url}}]]}
    });
    let client = reqwest::Client::new();
    if let Err(e) = client.post(format!("https://api.telegram.org/bot{token}/sendMessage")).json(&body).send().await {
        tracing::warn!(error = %e, "telegram push failed");
    }
}

/// Once a day after 20:00 local, if the day had any life: the yard writes its chronicle.
pub async fn maybe_chronicle(app: &AppState) -> Result<()> {
    let hour = chrono::Local::now().hour();
    if hour < 20 {
        return Ok(());
    }
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    if db::meta_get(&app.db, "chronicle_date").await?.as_deref() == Some(today.as_str()) {
        return Ok(());
    }
    if app.turn_lock.try_lock().is_err() {
        return Ok(());
    }
    db::meta_set(&app.db, "chronicle_date", &today).await?;
    let _turn = app.turn_lock.lock().await;
    if let Some(id) = crate::organism::chronicle(app, false).await? {
        tracing::info!(room = %app.room_id, %id, "chronicle written");
        app.emit(RoomEvent::Done);
    }
    Ok(())
}

/// Files nobody has read yet get digested within a couple of minutes of upload, player present or not.
pub async fn maybe_digest(app: &AppState) -> Result<()> {
    let care = crate::organism::needs_care(app, 3).await?;
    let Some(c) = care.into_iter().find(|c| c.kind == "digest") else { return Ok(()) };
    if app.turn_lock.try_lock().is_err() {
        return Ok(());
    }
    let _turn = app.turn_lock.lock().await;
    let agents = db::list_agents(&app.db).await?;
    let Some(node) = db::get_node(&app.db, &c.node_id).await? else { return Ok(()) };
    let Some(agent) = crate::organism::pick_carer(&agents, &node, "digest") else { return Ok(()) };
    let (recs, _) = crate::organism::tend(app, &agent.id, &c.node_id, "digest").await?;
    tracing::info!(room = %app.room_id, agent = %agent.id, node = %c.node_id, actions = recs.len(), "file digested");
    app.emit(RoomEvent::Done);
    Ok(())
}

/// Poll RSS feeds every `FEEDS_EVERY_MIN` (default 180) minutes.
pub async fn maybe_poll_feeds(app: &AppState) -> Result<()> {
    let every: i64 = app.llm_settings().await.get("FEEDS_EVERY_MIN").and_then(|v| v.parse().ok()).unwrap_or(180);
    if every <= 0 {
        return Ok(());
    }
    let last = db::meta_get(&app.db, "feeds_last_poll").await?;
    if let Some(min) = last.as_deref().and_then(minutes_since) {
        if min < every {
            return Ok(());
        }
    }
    db::meta_set(&app.db, "feeds_last_poll", &Utc::now().to_rfc3339()).await?;
    let n = crate::integrations::feeds::poll(app).await?;
    if n > 0 {
        tracing::info!(room = %app.room_id, new = n, "feeds: new bookmarks");
        let nodes = db::list_nodes(&app.db).await?;
        let edges = db::list_edges(&app.db).await?;
        let mermaid = crate::mermaid::build_graph(&nodes, &edges);
        app.emit(RoomEvent::Graph { graph: crate::models::Graph { nodes, edges }, mermaid });
    }
    Ok(())
}

/// Reset the away-backoff when the player returns.
pub async fn player_is_back(app: &AppState) -> Result<()> {
    db::meta_set(&app.db, "beats_while_away", "0").await
}
