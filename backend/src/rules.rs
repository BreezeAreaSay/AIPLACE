//! Automations: objects of type `rule` are small "when … do …" notes the heartbeat
//! evaluates. No DSL to learn — every knob is a property on the object:
//!
//! * `when`  — `по расписанию` | `просроченные задачи` | `новый объект` | `давно не трогали` | `игрок молчит`
//! * `every` — `час` | `день` | `неделя` (for schedules and quiet checks)
//! * `type`  — type id filter for `новый объект` / `давно не трогали` (empty = any)
//! * `days`  — staleness / silence threshold in days
//! * `who`   — inhabitant id who acts (empty = whoever fits the object)
//! * `do`    — `сказать` | `спросить жителя` | `разобрать` | `тегнуть` | `создать` | `написать в ЛС` | `хроника` | `аудит`
//! * `param` — template for the message / tags / "type: title"; `{count}` `{list}` `{label}` `{date}` `{rule}` are filled in
//! * `active`, `last_run`, `runs`, `log`, `seen` — bookkeeping written by the runner
//!
//! Matching objects are remembered in `seen`, so a rule fires once per object.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};

use crate::db;
use crate::models::{Agent, GraphNode, RoomEvent, USER_ID};
use crate::AppState;

pub const RULE_TYPE: &str = "rule";

const WHEN_SCHEDULE: &str = "по расписанию";
const WHEN_OVERDUE: &str = "просроченные задачи";
const WHEN_NEW: &str = "новый объект";
const WHEN_STALE: &str = "давно не трогали";
const WHEN_QUIET: &str = "игрок молчит";

const DO_SAY: &str = "сказать";
const DO_ASK: &str = "спросить жителя";
const DO_TEND: &str = "разобрать";
const DO_TAG: &str = "тегнуть";
const DO_CREATE: &str = "создать";
const DO_DM: &str = "написать в ЛС";
const DO_CHRONICLE: &str = "хроника";
const DO_AUDIT: &str = "аудит";

pub const WHEN_OPTIONS: [&str; 5] = [WHEN_SCHEDULE, WHEN_OVERDUE, WHEN_NEW, WHEN_STALE, WHEN_QUIET];
pub const DO_OPTIONS: [&str; 8] = [DO_SAY, DO_ASK, DO_TEND, DO_TAG, DO_CREATE, DO_DM, DO_CHRONICLE, DO_AUDIT];

fn prop_str<'a>(n: &'a GraphNode, key: &str) -> &'a str {
    n.props.0.get(key).and_then(|v| v.as_str()).map(str::trim).unwrap_or("")
}

fn prop_num(n: &GraphNode, key: &str) -> Option<f64> {
    match n.props.0.get(key) {
        Some(Value::Number(x)) => x.as_f64(),
        Some(Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    }
}

fn now_str() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}

fn every_minutes(n: &GraphNode) -> i64 {
    match prop_str(n, "every") {
        "час" => 60,
        "неделя" => 7 * 1440,
        _ => 1440,
    }
}

/// Does an LLM call stand behind this action? (The heartbeat skips those without budget.)
pub fn needs_llm(rule: &GraphNode) -> bool {
    matches!(prop_str(rule, "do"), DO_ASK | DO_TEND | DO_CHRONICLE)
}

fn fill(template: &str, rule: &GraphNode, matched: &[GraphNode]) -> String {
    let list = matched.iter().take(8).map(|n| format!("«{}»", n.label)).collect::<Vec<_>>().join(", ");
    let more = if matched.len() > 8 { format!(" и ещё {}", matched.len() - 8) } else { String::new() };
    template
        .replace("{count}", &matched.len().to_string())
        .replace("{list}", &format!("{list}{more}"))
        .replace("{label}", matched.first().map(|n| n.label.as_str()).unwrap_or(""))
        .replace("{date}", &chrono::Local::now().format("%d.%m").to_string())
        .replace("{rule}", &rule.label)
}

fn seen_ids(rule: &GraphNode) -> Vec<String> {
    rule.props.0.get("seen").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}

/// Which objects (if any) satisfy the rule's `when` right now. `None` = the rule is not due.
fn evaluate(rule: &GraphNode, nodes: &[GraphNode], last_user_msg: Option<&str>, force: bool) -> Option<Vec<GraphNode>> {
    let when = prop_str(rule, "when");
    let last_run = parse_ts(prop_str(rule, "last_run"));
    let due = |mins: i64| force || last_run.map(|t| Utc::now() - t >= Duration::minutes(mins)).unwrap_or(true);
    let seen = seen_ids(rule);
    let type_filter = prop_str(rule, "type").to_lowercase();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let live = |n: &&GraphNode| n.archived == 0 && n.id != rule.id && n.type_id != RULE_TYPE && !seen.contains(&n.id);
    match when {
        WHEN_OVERDUE => {
            let v: Vec<GraphNode> = nodes
                .iter()
                .filter(live)
                .filter(|n| n.props.0.get("done").and_then(|d| d.as_bool()) != Some(true))
                .filter(|n| n.props.0.get("due").and_then(|d| d.as_str()).map(|d| &d[..d.len().min(10)] < today.as_str()).unwrap_or(false))
                .cloned()
                .collect();
            (!v.is_empty() || force).then_some(v)
        }
        WHEN_NEW => {
            let since = last_run.unwrap_or_else(|| Utc::now() - Duration::hours(24));
            let v: Vec<GraphNode> = nodes
                .iter()
                .filter(live)
                .filter(|n| type_filter.is_empty() || n.type_id == type_filter)
                .filter(|n| n.created_by != "system")
                .filter(|n| parse_ts(&n.created_at).map(|t| t > since).unwrap_or(false))
                .cloned()
                .collect();
            (!v.is_empty() || force).then_some(v)
        }
        WHEN_STALE => {
            let days = prop_num(rule, "days").unwrap_or(14.0).max(1.0);
            let cutoff = Utc::now() - Duration::minutes((days * 1440.0) as i64);
            let v: Vec<GraphNode> = nodes
                .iter()
                .filter(live)
                .filter(|n| type_filter.is_empty() || n.type_id == type_filter)
                .filter(|n| n.type_id != "feed")
                .filter(|n| parse_ts(&n.updated_at).map(|t| t < cutoff).unwrap_or(false))
                .cloned()
                .collect();
            (!v.is_empty() || force).then_some(v)
        }
        WHEN_QUIET => {
            let days = prop_num(rule, "days").unwrap_or(3.0).max(0.5);
            let silent = last_user_msg.and_then(parse_ts).map(|t| Utc::now() - t >= Duration::minutes((days * 1440.0) as i64)).unwrap_or(true);
            ((silent && due(every_minutes(rule))) || force).then(Vec::new)
        }
        _ => due(every_minutes(rule)).then(Vec::new),
    }
}

fn pick_agent<'a>(agents: &'a [Agent], rule: &GraphNode, focus: Option<&GraphNode>) -> Option<&'a Agent> {
    let who = prop_str(rule, "who").to_lowercase();
    if !who.is_empty() {
        if let Some(a) = agents.iter().find(|a| a.is_alive() && (a.id == who || a.name.to_lowercase().starts_with(&who))) {
            return Some(a);
        }
    }
    match focus {
        Some(n) => crate::organism::pick_carer(agents, n, "manual"),
        None => agents.iter().filter(|a| a.is_alive()).max_by_key(|a| a.dopamine),
    }
}

/// Run one rule. Returns a one-line summary of what happened, or `None` if it was not due.
pub async fn run_rule(app: &AppState, rule: &GraphNode, force: bool, llm_ok: bool) -> Result<Option<String>> {
    let active = rule.props.0.get("active").and_then(|a| a.as_bool()).unwrap_or(true);
    if (!active && !force) || rule.archived != 0 {
        return Ok(None);
    }
    if needs_llm(rule) && !llm_ok && !force {
        return Ok(None);
    }
    let nodes = db::list_nodes(&app.db).await?;
    let last_user = db::last_user_message_at(&app.db).await?;
    let Some(matched) = evaluate(rule, &nodes, last_user.as_deref(), force) else { return Ok(None) };
    let when = prop_str(rule, "when");
    let object_based = matches!(when, WHEN_OVERDUE | WHEN_NEW | WHEN_STALE);
    if object_based && matched.is_empty() {
        let note = "условие не выполнено: подходящих объектов нет".to_string();
        bookkeep(app, rule, &matched, &note, false).await?;
        return Ok(Some(note));
    }
    let agents = db::list_agents(&app.db).await?;
    let agent = pick_agent(&agents, rule, matched.first());
    let actor = agent.map(|a| a.id.as_str()).unwrap_or("system");
    let param = prop_str(rule, "param");
    let text = fill(if param.is_empty() { default_param(when) } else { param }, rule, &matched);
    let focus = matched.first().map(|n| n.id.clone()).or_else(|| Some(rule.id.clone()));
    let result = match prop_str(rule, "do") {
        DO_SAY | "" => {
            let msg = db::insert_message_in(&app.db, "", actor, None, &text, Some(USER_ID), Some("правило"), focus.as_deref(), None).await?;
            for n in matched.iter().take(10) {
                db::add_mention(&app.db, msg.id, &n.id).await?;
            }
            let msg = db::get_message(&app.db, msg.id).await?.unwrap_or(msg);
            app.emit(RoomEvent::Message { message: msg });
            format!("сказано в комнате: {}", crate::mermaid::truncate_chars(&text, 80))
        }
        DO_DM => {
            let a = agent.context("для ЛС нужен житель (поле who)")?;
            let msg = db::insert_message_in(&app.db, &a.id, &a.id, None, &text, Some(USER_ID), Some("правило"), focus.as_deref(), None).await?;
            app.emit(RoomEvent::Message { message: msg });
            format!("{} написал в ЛС", a.name)
        }
        DO_ASK => {
            let a = agent.context("некому отвечать")?;
            let prompt = format!("⚙ правило «{}»: {text}", rule.label);
            let sys = db::insert_message_in(&app.db, "", "system", None, &prompt, Some(&a.id), Some("правило"), focus.as_deref(), None).await?;
            let sys_id = sys.id;
            app.emit(RoomEvent::Message { message: sys });
            app.emit(RoomEvent::Status { stage: "agent".into(), detail: format!("{} отвечает по правилу…", a.name) });
            let plan = crate::agent_runner::TurnPlan { beat: None, scene_so_far: vec![], room_mood: None, focus_node: focus.as_deref(), library: None, pre: None, pre_records: vec![], channel: None, reply_to: Some(sys_id) };
            let reply = crate::agent_runner::run_agent(app, &a.id, plan).await?;
            app.emit(RoomEvent::Done);
            format!("{}: {}", a.name, crate::mermaid::truncate_chars(&reply.content, 80))
        }
        DO_TEND => {
            let mut done = Vec::new();
            for n in matched.iter().take(3) {
                let a = pick_agent(&agents, rule, Some(n)).context("некому разбирать")?;
                app.emit(RoomEvent::Status { stage: "care".into(), detail: format!("{} разбирает «{}»", a.name, n.label) });
                match crate::organism::tend(app, &a.id, &n.id, "manual").await {
                    Ok(_) => done.push(n.label.clone()),
                    Err(e) => tracing::warn!(rule = %rule.label, node = %n.id, error = %e, "rule tend failed"),
                }
            }
            app.emit(RoomEvent::Done);
            if matched.is_empty() {
                "разбирать нечего".into()
            } else {
                format!("разобрано: {}", done.join(", "))
            }
        }
        DO_TAG => {
            let tags: Vec<Value> = param.split(',').map(|t| t.trim().trim_start_matches('#')).filter(|t| !t.is_empty()).map(|t| json!(t)).collect();
            if tags.is_empty() {
                anyhow::bail!("в param нужны теги через запятую");
            }
            for n in &matched {
                let mut cur: Vec<Value> = n.props.0.get("tags").and_then(|t| t.as_array()).cloned().unwrap_or_default();
                for t in &tags {
                    if !cur.contains(t) {
                        cur.push(t.clone());
                    }
                }
                db::update_object_meta(&app.db, &n.id, None, Some(&json!({ "tags": cur })), None).await?;
            }
            crate::handlers::emit_graph_pub(app).await?;
            format!("тегнуто объектов: {}", matched.len())
        }
        DO_CREATE => {
            let (type_id, title) = match text.split_once(':') {
                Some((t, rest)) if !t.trim().contains(' ') => (t.trim().to_lowercase(), rest.trim().to_string()),
                _ => ("note".to_string(), text.clone()),
            };
            let body = format!("Создано правилом «{}» {}.", rule.label, chrono::Local::now().format("%d.%m.%Y"));
            let (res, _) = crate::tools::call(app, actor, "library_create", &json!({ "type": type_id, "title": title, "body": body })).await?;
            if let (Some(id), Some(n)) = (res.get("id").and_then(|i| i.as_str()), matched.first()) {
                let _ = crate::tools::call(app, actor, "library_link", &json!({ "from": id, "to": n.id, "kind": "about", "label": "по правилу" })).await;
            }
            crate::handlers::emit_graph_pub(app).await?;
            format!("создано: «{title}»")
        }
        DO_CHRONICLE => match crate::organism::chronicle(app, true).await? {
            Some(id) => format!("хроника записана: {id}"),
            None => "хроника: день пустой".into(),
        },
        DO_AUDIT => format!("аудит: предложений {}", crate::organism::audit(app).await?),
        other => anyhow::bail!("неизвестное действие «{other}»"),
    };
    bookkeep(app, rule, &matched, &result, true).await?;
    tracing::info!(rule = %rule.label, result = %result, "rule fired");
    Ok(Some(result))
}

fn default_param(when: &str) -> &'static str {
    match when {
        WHEN_OVERDUE => "Просрочено {count}: {list}. Что с этим делаем?",
        WHEN_NEW => "В библиотеке новое: {list}.",
        WHEN_STALE => "Давно не трогали: {list}. Живо ещё?",
        WHEN_QUIET => "Игрок молчит который день. Подаём голос?",
        _ => "Правило «{rule}» сработало ({date}).",
    }
}

async fn bookkeep(app: &AppState, rule: &GraphNode, matched: &[GraphNode], log: &str, fired: bool) -> Result<()> {
    let mut seen = seen_ids(rule);
    for n in matched {
        if !seen.contains(&n.id) {
            seen.push(n.id.clone());
        }
    }
    if seen.len() > 300 {
        seen.drain(..seen.len() - 300);
    }
    let runs = prop_num(rule, "runs").unwrap_or(0.0) as i64 + i64::from(fired);
    let props = json!({ "last_run": now_str(), "runs": runs, "log": format!("{} — {log}", chrono::Local::now().format("%d.%m %H:%M")), "seen": seen });
    db::update_object_meta(&app.db, &rule.id, None, Some(&props), None).await?;
    crate::handlers::emit_graph_pub(app).await
}

/// Evaluate every active rule in the room (called by the heartbeat). Returns the summaries of
/// the rules that fired.
pub async fn run_all(app: &AppState, llm_ok: bool) -> Result<Vec<String>> {
    let rules: Vec<GraphNode> = db::list_nodes(&app.db).await?.into_iter().filter(|n| n.type_id == RULE_TYPE && n.archived == 0).collect();
    if rules.is_empty() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    for r in rules {
        let _turn = app.turn_lock.lock().await;
        match run_rule(app, &r, false, llm_ok).await {
            Ok(Some(s)) => out.push(format!("{}: {s}", r.label)),
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(rule = %r.label, error = %e, "rule failed");
                let _ = db::update_object_meta(&app.db, &r.id, None, Some(&json!({ "log": format!("ошибка: {e:#}") })), None).await;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, type_id: &str, props: Value, created: &str, updated: &str) -> GraphNode {
        GraphNode {
            id: id.into(),
            label: id.into(),
            node_type: "task".into(),
            created_by: USER_ID.into(),
            created_at: created.into(),
            body: String::new(),
            x: Some(0.0),
            y: Some(0.0),
            pinned: 0,
            updated_at: updated.into(),
            type_id: type_id.into(),
            props: sqlx::types::Json(props),
            archived: 0,
        }
    }

    #[test]
    fn overdue_rule_matches_once() {
        let rule = node("r", RULE_TYPE, json!({ "when": WHEN_OVERDUE }), "2026-01-01T00:00:00.000Z", "2026-01-01T00:00:00.000Z");
        let old = node("t1", "task", json!({ "due": "2020-01-01", "done": false }), "2026-01-01T00:00:00.000Z", "2026-01-01T00:00:00.000Z");
        let done = node("t2", "task", json!({ "due": "2020-01-01", "done": true }), "2026-01-01T00:00:00.000Z", "2026-01-01T00:00:00.000Z");
        let m = evaluate(&rule, &[old.clone(), done], None, false).unwrap();
        assert_eq!(m.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), vec!["t1"]);
        let seen = node("r", RULE_TYPE, json!({ "when": WHEN_OVERDUE, "seen": ["t1"] }), "2026-01-01T00:00:00.000Z", "2026-01-01T00:00:00.000Z");
        assert!(evaluate(&seen, &[old], None, false).is_none());
    }

    #[test]
    fn schedule_respects_every() {
        let fresh = now_str();
        let rule = node("r", RULE_TYPE, json!({ "when": WHEN_SCHEDULE, "every": "день", "last_run": fresh }), "", "");
        assert!(evaluate(&rule, &[], None, false).is_none());
        let never = node("r", RULE_TYPE, json!({ "when": WHEN_SCHEDULE }), "", "");
        assert!(matches!(evaluate(&never, &[], None, false), Some(v) if v.is_empty()));
    }

    #[test]
    fn templates_fill() {
        let rule = node("r", RULE_TYPE, json!({}), "", "");
        let a = node("a", "task", json!({}), "", "");
        assert_eq!(fill("{count}: {list} / {label}", &rule, &[a]), "1: «a» / a");
    }
}
