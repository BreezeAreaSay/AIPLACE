//! Calendar both ways: the library's dated things as an `.ics` feed you subscribe to in
//! Google/Яндекс/Apple Calendar, and your own private `.ics` URL read back so the room knows
//! what's coming (next 7 days, cached 30 min).

use std::{
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use anyhow::Result;
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde::Serialize;

use super::http;
use crate::{db, state::AppState};

#[derive(Debug, Clone, Serialize)]
pub struct CalEvent {
    pub when: String,
    pub all_day: bool,
    pub title: String,
    pub place: String,
    pub source: String,
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace('\n', "\\n")
}

fn date_of(v: &serde_json::Value) -> Option<(NaiveDate, Option<NaiveDateTime>)> {
    let s = v.as_str()?.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        let n = dt.naive_local();
        return Some((n.date(), Some(n)));
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s.get(0..16)?, "%Y-%m-%dT%H:%M") {
        return Some((dt.date(), Some(dt)));
    }
    NaiveDate::parse_from_str(s.get(0..10)?, "%Y-%m-%d").ok().map(|d| (d, None))
}

/// Everything in the library that has a date: tasks (due), events (date), goals (due),
/// projects (due), diary entries, open contracts (deadline).
pub async fn ics(app: &AppState) -> Result<String> {
    let nodes = db::list_nodes(&app.db).await?;
    let contracts = db::open_contracts(&app.db).await?;
    let mut out = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Triad Canvas//RU\r\nCALSCALE:GREGORIAN\r\nX-WR-CALNAME:Triad Canvas\r\n");
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let mut push = |uid: &str, date: NaiveDate, time: Option<NaiveDateTime>, title: &str, desc: &str| {
        out.push_str("BEGIN:VEVENT\r\n");
        out.push_str(&format!("UID:{uid}@triad-canvas\r\nDTSTAMP:{stamp}\r\n"));
        match time {
            Some(t) => {
                out.push_str(&format!("DTSTART:{}\r\nDTEND:{}\r\n", t.format("%Y%m%dT%H%M%S"), (t + chrono::Duration::hours(1)).format("%Y%m%dT%H%M%S")));
            }
            None => {
                out.push_str(&format!("DTSTART;VALUE=DATE:{}\r\nDTEND;VALUE=DATE:{}\r\n", date.format("%Y%m%d"), (date + chrono::Duration::days(1)).format("%Y%m%d")));
            }
        }
        out.push_str(&format!("SUMMARY:{}\r\n", esc(title)));
        if !desc.is_empty() {
            out.push_str(&format!("DESCRIPTION:{}\r\n", esc(desc)));
        }
        out.push_str("END:VEVENT\r\n");
    };
    for n in nodes.iter().filter(|n| n.archived == 0) {
        let p = &n.props.0;
        let (key, prefix) = match n.type_id.as_str() {
            "task" => ("due", "☐ "),
            "event" => ("date", "📅 "),
            "goal" => ("due", "◎ "),
            "project" => ("due", "◆ "),
            "diary" => ("date", "📓 "),
            "human" => ("birthday", "🎂 "),
            _ => continue,
        };
        if n.type_id == "task" && p.get("done").and_then(|d| d.as_bool()) == Some(true) {
            continue;
        }
        let Some((d, t)) = date_of(&p[key]) else { continue };
        let desc = format!("{}{}", crate::mermaid::truncate_chars(n.body.trim(), 300), p.get("place").and_then(|x| x.as_str()).map(|x| format!("\n{x}")).unwrap_or_default());
        push(&n.id, d, t, &format!("{prefix}{}", n.label), &desc);
    }
    for c in &contracts {
        if let Some((d, t)) = c.deadline.as_deref().and_then(|s| date_of(&serde_json::Value::String(s.to_string()))) {
            push(&format!("contract-{}", c.id), d, t, &format!("⚑ Слово: {}", c.title), c.details.as_deref().unwrap_or(""));
        }
    }
    out.push_str("END:VCALENDAR\r\n");
    Ok(out)
}

// ───────────── reading your calendar ─────────────

struct Cache {
    url: String,
    at: Instant,
    events: Vec<CalEvent>,
}
fn cache() -> &'static Mutex<Option<Cache>> {
    static C: OnceLock<Mutex<Option<Cache>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(None))
}

fn unfold(ics: &str) -> String {
    ics.replace("\r\n ", "").replace("\n ", "").replace("\r\n\t", "").replace("\n\t", "")
}

fn parse_dt(raw: &str) -> Option<(NaiveDateTime, bool)> {
    let v = raw.trim();
    if v.len() == 8 {
        return NaiveDate::parse_from_str(v, "%Y%m%d").ok().map(|d| (d.and_hms_opt(0, 0, 0).unwrap(), true));
    }
    let v = v.trim_end_matches('Z');
    NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok().map(|t| (t, false))
}

/// Minimal VEVENT parser: DTSTART / SUMMARY / LOCATION; recurrence is ignored except the
/// first instance (adequate for "what's on this week").
pub fn parse_ics(ics: &str, from: NaiveDateTime, to: NaiveDateTime, source: &str) -> Vec<CalEvent> {
    let text = unfold(ics);
    let mut out = vec![];
    for block in text.split("BEGIN:VEVENT").skip(1) {
        let body = block.split("END:VEVENT").next().unwrap_or("");
        let mut start: Option<(NaiveDateTime, bool)> = None;
        let mut summary = String::new();
        let mut place = String::new();
        for line in body.lines() {
            let line = line.trim();
            if let Some(v) = line.strip_prefix("DTSTART") {
                let v = v.rsplit(':').next().unwrap_or("");
                start = parse_dt(v);
            } else if let Some(v) = line.strip_prefix("SUMMARY") {
                summary = v.split_once(':').map(|x| x.1).unwrap_or("").replace("\\,", ",").replace("\\n", " ").trim().to_string();
            } else if let Some(v) = line.strip_prefix("LOCATION") {
                place = v.split_once(':').map(|x| x.1).unwrap_or("").replace("\\,", ",").trim().to_string();
            }
        }
        if let Some((t, all_day)) = start {
            if t >= from && t <= to && !summary.is_empty() {
                out.push(CalEvent { when: if all_day { t.format("%Y-%m-%d").to_string() } else { t.format("%Y-%m-%d %H:%M").to_string() }, all_day, title: summary, place, source: source.to_string() });
            }
        }
    }
    out.sort_by(|a, b| a.when.cmp(&b.when));
    out
}

/// Upcoming events: the library's own dated items plus your external calendar.
pub async fn upcoming(app: &AppState, days: i64) -> Result<Vec<CalEvent>> {
    let now = chrono::Local::now().naive_local();
    let to = now + chrono::Duration::days(days.clamp(1, 60));
    let mut events: Vec<CalEvent> = vec![];
    // library
    let own = ics(app).await?;
    events.extend(parse_ics(&own, now - chrono::Duration::hours(12), to, "библиотека"));
    // external
    let s = app.llm_settings().await;
    if let Some(url) = s.get("CALENDAR_ICS_URL") {
        let cached = cache().lock().unwrap().as_ref().filter(|c| c.url == url && c.at.elapsed() < Duration::from_secs(30 * 60)).map(|c| c.events.clone());
        let ext = match cached {
            Some(e) => e,
            None => {
                let url_fixed = url.replace("webcal://", "https://");
                match http().get(&url_fixed).send().await.and_then(|r| r.error_for_status()) {
                    Ok(r) => {
                        let body = r.text().await.unwrap_or_default();
                        let ev = parse_ics(&body, now - chrono::Duration::hours(12), now + chrono::Duration::days(60), "твой календарь");
                        *cache().lock().unwrap() = Some(Cache { url: url.to_string(), at: Instant::now(), events: ev.clone() });
                        ev
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "calendar fetch failed");
                        vec![]
                    }
                }
            }
        };
        events.extend(ext.into_iter().filter(|e| e.when.as_str() <= to.format("%Y-%m-%d %H:%M").to_string().as_str()));
    }
    events.sort_by(|a, b| a.when.cmp(&b.when));
    events.truncate(25);
    Ok(events)
}

pub fn digest(events: &[CalEvent]) -> String {
    if events.is_empty() {
        return "ближайшую неделю в календаре пусто".into();
    }
    let today = chrono::Local::now().date_naive();
    events
        .iter()
        .map(|e| {
            let d = NaiveDate::parse_from_str(e.when.get(0..10).unwrap_or(""), "%Y-%m-%d").ok();
            let rel = match d.map(|d| (d - today).num_days()) {
                Some(0) => "сегодня".to_string(),
                Some(1) => "завтра".to_string(),
                Some(n) if n > 1 => format!("через {n} дн., {}", d.map(|d| weekday_ru(d.weekday())).unwrap_or("")),
                _ => "прошло".to_string(),
            };
            format!("- {} ({rel}) {}{}{}", e.when, e.title, if e.place.is_empty() { String::new() } else { format!(" @ {}", e.place) }, if e.source == "библиотека" { String::new() } else { format!(" [{}]", e.source) })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn weekday_ru(w: chrono::Weekday) -> &'static str {
    match w {
        chrono::Weekday::Mon => "пн",
        chrono::Weekday::Tue => "вт",
        chrono::Weekday::Wed => "ср",
        chrono::Weekday::Thu => "чт",
        chrono::Weekday::Fri => "пт",
        chrono::Weekday::Sat => "сб",
        chrono::Weekday::Sun => "вс",
    }
}

#[allow(dead_code)]
fn _assert_tz() {
    let _ = Utc.timestamp_opt(0, 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vevents_in_window() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART;TZID=Europe/Moscow:20300105T100000\r\nSUMMARY:Встреча\\, важная\r\nLOCATION:Кафе\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20300107\r\nSUMMARY:День\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let from = NaiveDate::from_ymd_opt(2030, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap();
        let to = NaiveDate::from_ymd_opt(2030, 1, 6).unwrap().and_hms_opt(0, 0, 0).unwrap();
        let ev = parse_ics(ics, from, to, "x");
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].title, "Встреча, важная");
        assert_eq!(ev[0].place, "Кафе");
        assert_eq!(ev[0].when, "2030-01-05 10:00");
    }
}
