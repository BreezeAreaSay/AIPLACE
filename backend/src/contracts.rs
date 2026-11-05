//! Player promises ("слово"). An agent guarantees each one with their own HP: keeping it
//! feeds the room, breaking it hurts the guarantor and everyone's respect for the player.

use anyhow::Result;
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use rand::seq::SliceRandom;

use crate::{
    db, mermaid,
    models::{Agent, Contract, ContractUpdate, NewContract, RoomEvent},
    state::AppState,
};

pub const DEFAULT_STAKE: i64 = 15;
pub const MAX_STAKE: i64 = 40;
const MAX_OPEN: usize = 12;

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Accept RFC3339, `YYYY-MM-DDTHH:MM[:SS]` and bare `YYYY-MM-DD` (→ 23:59 UTC that day).
pub fn normalize_deadline(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("null") || t.eq_ignore_ascii_case("none") {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(t) {
        return Some(dt.with_timezone(&Utc).to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    }
    for fmt in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(t, fmt) {
            return Some(naive.and_utc().to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
        }
    }
    if let Ok(d) = NaiveDate::parse_from_str(t, "%Y-%m-%d") {
        let naive = d.and_hms_opt(23, 59, 0)?;
        return Some(naive.and_utc().to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    }
    None
}

fn pick_guarantor<'a>(raw: &str, agents: &'a [Agent]) -> Option<&'a Agent> {
    let alive: Vec<&Agent> = agents.iter().filter(|a| a.is_alive()).collect();
    let wanted = mermaid::normalize_author(raw);
    if wanted != "user" {
        if let Some(a) = alive.iter().find(|a| a.id == wanted) {
            return Some(a);
        }
    }
    let mut rng = rand::thread_rng();
    alive.choose(&mut rng).copied()
}

/// Called with the Director's `new_contracts`.
pub async fn apply_new(app: &AppState, items: &[NewContract], agents: &[Agent]) -> Result<Vec<Contract>> {
    let open = db::open_contracts(&app.db).await?;
    // Also refuse to re-create a word that was just closed (LLMs love to echo the last line).
    let recent_cutoff = (Utc::now() - chrono::Duration::hours(2)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let recent: Vec<Contract> = db::list_contracts(&app.db)
        .await?
        .into_iter()
        .filter(|c| c.status != "open" && c.created_at.as_str() >= recent_cutoff.as_str())
        .collect();
    let mut created = Vec::new();
    for c in items.iter().take(3) {
        if open.len() + created.len() >= MAX_OPEN {
            break;
        }
        let title = mermaid::truncate_chars(c.title.trim(), 120);
        if title.chars().count() < 4 {
            continue;
        }
        // Skip near-duplicates of open contracts.
        let lt = title.to_lowercase();
        if open.iter().chain(recent.iter()).chain(created.iter()).any(|o| o.title.to_lowercase() == lt) {
            continue;
        }
        let deadline = c.deadline.as_deref().and_then(normalize_deadline);
        let guarantor = pick_guarantor(&c.guarantor, agents);
        let stake = c
            .stake
            .map(|s| (s.round() as i64).clamp(5, MAX_STAKE))
            .unwrap_or(DEFAULT_STAKE);
        let contract = db::insert_contract(
            &app.db,
            &title,
            c.details.as_deref().map(str::trim).filter(|d| !d.is_empty()),
            deadline.as_deref(),
            guarantor.map(|g| g.id.as_str()),
            stake,
        )
        .await?;
        let who = guarantor.map(|g| g.name.clone()).unwrap_or_else(|| "никто".into());
        let when = contract.deadline.as_deref().map(|d| format!(" до {}", human_deadline(d))).unwrap_or_default();
        db::insert_message(
            &app.db,
            "system",
            None,
            &format!("🤝 Слово #{}: «{}»{when}. Поручитель — {who}, ставка {} HP.", contract.id, contract.title, contract.stake),
        )
        .await?;
        db::insert_event(
            &app.db,
            "contract_created",
            &serde_json::json!({
                "id": contract.id, "title": contract.title, "deadline": contract.deadline,
                "guarantor": contract.guarantor_id, "stake": contract.stake, "why": c.why,
            }),
        )
        .await?;
        created.push(contract);
    }
    Ok(created)
}

/// Director's verdicts on the player's reports ("сделал", "провалил").
pub async fn apply_updates(app: &AppState, items: &[ContractUpdate]) -> Result<()> {
    for u in items.iter().take(5) {
        let id = match &u.id {
            serde_json::Value::Number(n) => n.as_i64(),
            serde_json::Value::String(s) => s.trim_start_matches('#').parse().ok(),
            _ => None,
        };
        let Some(id) = id else { continue };
        let status = u.status.trim().to_lowercase();
        if !matches!(status.as_str(), "done" | "failed" | "cancelled") {
            continue;
        }
        resolve(app, id, &status, u.note.as_deref(), "director").await?;
    }
    Ok(())
}

pub fn human_deadline(rfc: &str) -> String {
    DateTime::parse_from_rfc3339(rfc)
        .map(|d| d.with_timezone(&Utc).format("%d.%m %H:%M UTC").to_string())
        .unwrap_or_else(|_| rfc.to_string())
}

/// Resolve a contract and apply consequences. `by` is `user` | `director` | `deadline`.
pub async fn resolve(app: &AppState, id: i64, status: &str, note: Option<&str>, by: &str) -> Result<Option<Contract>> {
    let Some(c) = db::get_contract(&app.db, id).await? else { return Ok(None) };
    if c.status != "open" {
        return Ok(Some(c));
    }
    db::resolve_contract(&app.db, id, status, note).await?;
    let agents = db::list_agents(&app.db).await?;
    let guarantor = c.guarantor_id.as_deref().and_then(|g| agents.iter().find(|a| a.id == g));
    let gname = guarantor.map(|g| g.name.clone()).unwrap_or_else(|| "поручитель".into());
    let mut deaths: Vec<String> = Vec::new();

    match status {
        "done" => {
            for a in agents.iter().filter(|a| a.is_alive()) {
                let is_g = guarantor.map(|g| g.id == a.id).unwrap_or(false);
                let hp = (a.hp + if is_g { c.stake / 2 } else { 2 }).min(100);
                let dop = (a.dopamine + if is_g { 15 } else { 6 }).min(100);
                let resp = (a.respect_user + if is_g { 12 } else { 5 }).min(100);
                db::update_agent_stats(&app.db, &a.id, hp, dop, resp).await?;
            }
            db::insert_message(
                &app.db,
                "system",
                None,
                &format!("✅ Слово #{} сдержано: «{}». {gname} получает +{} HP, комната — уважение.", c.id, c.title, c.stake / 2),
            )
            .await?;
        }
        "failed" => {
            for a in agents.iter().filter(|a| a.is_alive()) {
                let is_g = guarantor.map(|g| g.id == a.id).unwrap_or(false);
                let hp = (a.hp - if is_g { c.stake } else { 3 }).max(0);
                let dop = (a.dopamine - if is_g { 10 } else { 4 }).max(0);
                let resp = (a.respect_user - if is_g { 15 } else { 6 }).max(-100);
                db::update_agent_stats(&app.db, &a.id, hp, dop, resp).await?;
                if hp == 0 {
                    deaths.push(a.id.clone());
                    db::insert_message(&app.db, "system", None, &format!("☠ {} не пережил твоего нарушенного слова. HP = 0.", a.name)).await?;
                }
            }
            let how = match by {
                "deadline" => "дедлайн истёк",
                "director" => "по твоему же признанию",
                _ => "ты сам это признал",
            };
            db::insert_message(
                &app.db,
                "system",
                None,
                &format!("💀 Слово #{} нарушено ({how}): «{}». {gname} теряет {} HP.", c.id, c.title, c.stake),
            )
            .await?;
        }
        _ => {
            db::insert_message(&app.db, "system", None, &format!("✖ Слово #{} отозвано: «{}».", c.id, c.title)).await?;
        }
    }
    db::insert_event(
        &app.db,
        &format!("contract_{status}"),
        &serde_json::json!({
            "id": c.id, "title": c.title, "guarantor": c.guarantor_id, "stake": c.stake,
            "by": by, "note": note,
        }),
    )
    .await?;
    for id in deaths {
        db::insert_event(&app.db, "agent_died", &serde_json::json!({ "agent": id, "cause": format!("contract_{status}") })).await?;
    }
    app.emit(RoomEvent::Agents { agents: db::list_agents(&app.db).await? });
    emit_contracts(app).await?;
    db::get_contract(&app.db, id).await
}

/// Fail every open contract whose deadline has passed. Returns how many expired.
pub async fn expire_overdue(app: &AppState) -> Result<usize> {
    let now = now_rfc3339();
    let open = db::open_contracts(&app.db).await?;
    let mut n = 0;
    for c in open {
        if let Some(d) = &c.deadline {
            if d.as_str() < now.as_str() {
                resolve(app, c.id, "failed", Some("дедлайн истёк"), "deadline").await?;
                n += 1;
            }
        }
    }
    Ok(n)
}

pub async fn emit_contracts(app: &AppState) -> Result<()> {
    app.emit(RoomEvent::Contracts { contracts: db::list_contracts(&app.db).await? });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadlines_normalize() {
        assert_eq!(normalize_deadline("2026-11-07").as_deref(), Some("2026-11-07T23:59:00Z"));
        assert_eq!(normalize_deadline("2026-11-07T18:00").as_deref(), Some("2026-11-07T18:00:00Z"));
        assert_eq!(normalize_deadline("2026-11-07T18:00:00+03:00").as_deref(), Some("2026-11-07T15:00:00Z"));
        assert!(normalize_deadline("завтра").is_none());
    }
}
