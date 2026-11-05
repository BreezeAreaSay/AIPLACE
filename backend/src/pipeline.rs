//! Sub-agent pipeline: the Director's job split into small calls with small JSON outputs.
//! Reader → Caster run before anyone speaks; Cartographer ∥ Psychologist run after the round.
//! Every stage has a deterministic fallback, so a weak or flaky model never stalls the room.

use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    contracts, db,
    director::Trigger,
    mermaid,
    models::GraphNode,
    models::{Beat, ContractUpdate, DirectorOutput, GraphMutations, NewContract, NodeUpdate, RelationDelta, StatDelta},
    openrouter::{extract_json, ChatMessage, ChatOpts},
    prompts,
    state::AppState,
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StageLog {
    pub stage: String,
    pub ok: bool,
    pub ms: u128,
    #[serde(default)]
    pub error: Option<String>,
}

fn opts(max_tokens: u32) -> ChatOpts {
    ChatOpts { temperature: 0.7, max_tokens, json_mode: true, prefill: None, presence_penalty: None, frequency_penalty: None }
}

async fn call_json(app: &AppState, stage: &str, system: String, user: String, max_tokens: u32, log: &mut Vec<StageLog>) -> Option<Value> {
    let model = app.llm_settings().await.director_model;
    let t = Instant::now();
    let msgs = [ChatMessage::system(system), ChatMessage::user(user)];
    let res = app.llm.chat(&model, &msgs, &opts(max_tokens)).await.and_then(|raw| extract_json(&raw));
    match res {
        Ok(v) => {
            log.push(StageLog { stage: stage.into(), ok: true, ms: t.elapsed().as_millis(), error: None });
            Some(v)
        }
        Err(e) => {
            tracing::warn!(stage, error = %e, "pipeline stage failed → fallback");
            log.push(StageLog { stage: stage.into(), ok: false, ms: t.elapsed().as_millis(), error: Some(mermaid::truncate_chars(&e.to_string(), 200)) });
            None
        }
    }
}

/// What the pre-speech half of the pipeline produced; the handler keeps it to finish the round.
pub struct RoundPlan {
    pub output: DirectorOutput,
    pub reader: Value,
    pub stages: Vec<StageLog>,
    pub first_round_message_id: i64,
    /// Library digest answering the player's memory question, if any.
    pub library: Option<String>,
}

/// Reader → contracts → Caster. Returns the plan with a staged scene.
pub async fn before_speech(app: &AppState, trigger: &Trigger) -> Result<RoundPlan> {
    let mut stages = Vec::new();
    let agents = db::list_agents(&app.db).await?;
    let messages = db::recent_messages(&app.db, app.cfg.context_messages).await?;
    let open = db::open_contracts(&app.db).await?;
    let nodes = db::list_nodes(&app.db).await?;
    let relations = db::list_relations(&app.db).await?;
    let events = db::pending_events(&app.db).await?;
    let memories = db::list_memories(&app.db).await?;
    let now = contracts::now_rfc3339();
    let last_user_msg = messages.iter().rev().find(|m| m.sender_id == crate::models::USER_ID).cloned();
    let last_user = last_user_msg.as_ref().map(|m| m.content.clone()).unwrap_or_default();
    // The round starts with the player's line (so the Cartographer sees it) or, on ticks, now.
    let first_round_message_id = match (trigger, &last_user_msg) {
        (Trigger::UserMessage | Trigger::Node { .. }, Some(m)) => m.id - 1,
        _ => db::last_message_id(&app.db).await?,
    };

    // ── Reader (only when the player actually said something new) ──
    let mut reader = json!({ "topic": mermaid::truncate_chars(&last_user, 40), "feeds": {}, "promises": [], "contract_reports": [], "node_refs": [] });
    if matches!(trigger, Trigger::UserMessage | Trigger::Node { .. }) && !last_user.is_empty() {
        let recent: Vec<_> = messages.iter().rev().take(6).rev().cloned().collect();
        if let Some(v) = call_json(app, "reader", prompts::reader_system(), prompts::reader_user(&now, &last_user, &recent, &agents, &open, &nodes), 500, &mut stages).await {
            reader = v;
        }
    }
    // Memory question → deterministic library lookup, handed to the speakers as a digest.
    let mut library: Option<String> = None;
    if let Some(q) = reader.get("memory_query").filter(|q| !q.is_null()) {
        let text = q.get("text").and_then(|t| t.as_str()).unwrap_or(&last_user).to_string();
        let type_id = q.get("type").and_then(|t| t.as_str()).filter(|t| !t.is_empty() && *t != "null").map(str::to_string);
        let hits = db::search_objects(&app.db, &text, type_id.as_deref(), 8).await?;
        let types = db::list_types(&app.db).await?;
        let open_words: Vec<String> = db::open_contracts(&app.db).await?.iter().map(|c| format!("- [Слово #{}] «{}» до {} (поручитель {})", c.id, c.title, c.deadline.as_deref().map(contracts::human_deadline).unwrap_or_else(|| "без срока".into()), c.guarantor_id.clone().unwrap_or_default())).collect();
        let mut digest = prompts::library_digest(&hits, &types, &agents);
        if !open_words.is_empty() {
            digest.push('\n');
            digest.push_str(&open_words.join("\n"));
        }
        library = Some(digest);
        stages.push(StageLog { stage: "library".into(), ok: true, ms: 0, error: None });
    }
    // Contracts from the Reader: the most skeptical living agent guarantees.
    let mut new_contracts: Vec<NewContract> = Vec::new();
    if let Some(arr) = reader.get("promises").and_then(|p| p.as_array()) {
        let guarantor = agents.iter().filter(|a| a.is_alive()).min_by_key(|a| a.respect_user).map(|a| a.id.clone()).unwrap_or_default();
        for p in arr.iter().take(2) {
            if let Some(title) = p.get("title").and_then(|t| t.as_str()) {
                new_contracts.push(NewContract {
                    title: title.to_string(),
                    details: None,
                    deadline: p.get("deadline").and_then(|d| d.as_str()).map(str::to_string),
                    guarantor: guarantor.clone(),
                    stake: Some(15.0),
                    why: Some("самый недоверчивый ставит первым".into()),
                });
            }
        }
    }
    let contract_updates: Vec<ContractUpdate> = reader
        .get("contract_reports")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|u| Some(ContractUpdate { id: u.get("id")?.clone(), status: u.get("status")?.as_str()?.to_string(), note: None }))
                .collect()
        })
        .unwrap_or_default();
    contracts::apply_updates(app, &contract_updates).await?;
    let fresh = db::list_agents(&app.db).await?;
    let created = contracts::apply_new(app, &new_contracts, &fresh).await?;
    if !created.is_empty() {
        contracts::emit_contracts(app).await?;
    }

    // ── Caster ──
    let focus_node = match trigger {
        Trigger::Node { id, .. } => db::get_node(&app.db, id).await?,
        _ => None,
    };
    let focus = match (&focus_node, trigger) {
        (Some(n), Trigger::Node { mode, .. }) => Some((n, mode.as_str())),
        _ => None,
    };
    let recent_speakers = crate::director::recent_speakers(&messages, &agents);
    let life_kind = match trigger {
        Trigger::Life(k) => Some(k.as_str()),
        _ => None,
    };
    let mut own_projects: Vec<(String, String, String)> = Vec::new();
    if matches!(trigger, Trigger::Life(_) | Trigger::Tick) {
        for a in fresh.iter().filter(|a| a.is_alive()) {
            for n in db::objects_by_owner(&app.db, &a.id).await?.into_iter().filter(|n| n.type_id == "project").take(1) {
                own_projects.push((a.id.clone(), n.label.clone(), n.body.clone()));
            }
        }
    }
    let world = crate::integrations::world_line(app).await;
    let calendar = if matches!(trigger, Trigger::UserMessage | Trigger::Life(_)) { crate::integrations::calendar_line(app).await } else { None };
    let news: Vec<(String, String, String)> = if matches!(trigger, Trigger::Life(crate::heartbeat::LifeKind::News)) {
        crate::integrations::feeds::fresh_news(app, 6).await?.into_iter().map(|n| (n.label.clone(), n.props.0.get("summary").and_then(|x| x.as_str()).unwrap_or("").to_string(), n.props.0.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string())).collect()
    } else {
        vec![]
    };
    let focus_files: Option<String> = match &focus_node {
        Some(n) => {
            let atts = db::attachments_of(&app.db, &n.id).await?;
            if atts.is_empty() { None } else { Some(crate::files::digest(&atts, 500)) }
        }
        None => None,
    };
    let caster_in = prompts::CasterInput {
        trigger: trigger.as_str(),
        life_kind,
        own_projects: &own_projects,
        world: world.as_deref(),
        calendar: calendar.as_deref(),
        news: &news,
        focus_files: focus_files.as_deref(),
        agents: &fresh,
        recent: &messages,
        recent_speakers: &recent_speakers,
        reader: &reader,
        relations: &relations,
        events: &db::pending_events(&app.db).await?,
        focus,
        memories: &memories,
    };
    let caster = call_json(app, "caster", prompts::caster_system(), prompts::caster_user(&caster_in), 900, &mut stages).await;

    let mut output = DirectorOutput::default();
    if let Some(v) = &caster {
        output.room_mood = v.get("room_mood").and_then(|x| x.as_str()).map(str::to_string);
        output.director_note = v.get("note").and_then(|x| x.as_str()).map(str::to_string);
        output.scene = v.get("scene").cloned().and_then(|s| serde_json::from_value::<Vec<Beat>>(s).ok()).unwrap_or_default();
        output.updated_agendas = v.get("agendas").cloned().and_then(|a| serde_json::from_value(a).ok()).unwrap_or_default();
    } else {
        output.director_note = Some("Постановщик не ответил: сцена поставлена по ротации.".into());
    }
    output.new_contracts = new_contracts;
    output.contract_updates = contract_updates;
    crate::director::normalize_scene(&mut output, &fresh, &recent_speakers);
    // A freshly appointed guarantor opens the scene.
    for c in &created {
        if let Some(g) = &c.guarantor_id {
            if !output.scene.iter().any(|b| &b.speaker == g) {
                output.scene.insert(0, Beat {
                    speaker: g.clone(),
                    addressee: Some("user".into()),
                    r#move: Some("предложение".into()),
                    intent: Some(format!("принять ставку по слову #{} «{}» и назвать цену провала", c.id, c.title)),
                    delivery: Some("коротко, с нажимом".into()),
                });
                output.scene.truncate(4);
                output.next_speakers = crate::director::derive_speakers(&output.scene);
            }
        }
    }
    crate::director::apply_agendas_pub(app, &output, &fresh).await?;
    if let Some(last) = events.last() {
        db::mark_events_narrated(&app.db, last.id).await?;
    }
    output.stages = stages.clone();
    Ok(RoundPlan { output, reader, stages, first_round_message_id, library })
}

/// Cartographer ∥ Psychologist on the lines spoken this round; applies everything and
/// returns the completed DirectorOutput (for the log) plus stage timings.
pub async fn after_speech(app: &AppState, trigger: &Trigger, plan: &mut RoundPlan) -> Result<()> {
    let agents = db::list_agents(&app.db).await?;
    let until = db::last_message_id(&app.db).await?;
    let round = db::messages_between(&app.db, plan.first_round_message_id, until, 20).await?;
    let nodes = db::list_nodes(&app.db).await?;
    let edges = db::list_edges(&app.db).await?;
    let relations = db::list_relations(&app.db).await?;
    let events = db::recent_events(&app.db, 4).await?;
    let focus = match trigger {
        Trigger::Node { id, .. } => db::get_node(&app.db, id).await?,
        _ => None,
    };

    let types = db::list_types(&app.db).await?;
    // Candidates for dedup: recent objects + anything sharing a word with the round.
    let round_text = round.iter().map(|m| m.content.to_lowercase()).collect::<Vec<_>>().join(" ");
    let mut candidates: Vec<GraphNode> = nodes.iter().rev().take(25).cloned().collect();
    for n in nodes.iter() {
        if candidates.len() >= 60 {
            break;
        }
        if candidates.iter().any(|c| c.id == n.id) {
            continue;
        }
        let hit = n.label.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() >= 4).any(|w| round_text.contains(w));
        if hit {
            candidates.push(n.clone());
        }
    }
    let _ = &edges;

    let mut log_a = Vec::new();
    let mut log_b = Vec::new();
    let (arch, psych) = tokio::join!(
        call_json(app, "archivist", prompts::archivist_system(), prompts::archivist_user(&round, &agents, &types, &candidates, focus.as_ref()), 1200, &mut log_a),
        call_json(app, "psychologist", prompts::psychologist_system(), prompts::psychologist_user(&round, &agents, &relations, &plan.reader, &events), 900, &mut log_b),
    );
    plan.stages.extend(log_a);
    plan.stages.extend(log_b);

    if let Some(v) = arch {
        apply_archivist(app, &v, &round, &types).await?;
        // Mirror into the legacy DirectorOutput shape for the inspector.
        plan.output.graph_mutations = GraphMutations {
            nodes: v.get("objects").and_then(|o| o.as_array()).map(|arr| arr.iter().filter_map(|o| Some(crate::models::NewNode {
                id: o.get("id")?.as_str()?.to_string(),
                label: o.get("title")?.as_str()?.to_string(),
                node_type: o.get("type").and_then(|t| t.as_str()).unwrap_or("note").to_string(),
                author: o.get("author").and_then(|a| a.as_str()).unwrap_or("user").to_string(),
                body: o.get("body").and_then(|b| b.as_str()).map(str::to_string),
            })).collect()).unwrap_or_default(),
            edges: v.get("relations").and_then(|r| r.as_array()).map(|arr| arr.iter().filter_map(|r| Some(crate::models::NewEdge {
                from: r.get("from")?.as_str()?.to_string(),
                to: r.get("to")?.as_str()?.to_string(),
                label: r.get("label").and_then(|l| l.as_str()).unwrap_or("").to_string(),
                author: None,
            })).collect()).unwrap_or_default(),
        };
        plan.output.node_updates = Vec::<NodeUpdate>::new();
    }
    if let Some(v) = psych {
        plan.output.inner_states = v.get("inner_states").cloned().and_then(|x| serde_json::from_value(x).ok()).unwrap_or_default();
        // Next-round agendas for everyone (speakers already had fresh ones from the Caster).
        let next: std::collections::HashMap<String, String> = v.get("agendas").cloned().and_then(|x| serde_json::from_value(x).ok()).unwrap_or_default();
        for (k, val) in next {
            plan.output.updated_agendas.insert(k, val);
        }
        plan.output.stat_deltas = v.get("stat_deltas").cloned().and_then(|x| serde_json::from_value::<std::collections::HashMap<String, StatDelta>>(x).ok()).unwrap_or_default();
        plan.output.relation_deltas = v.get("relation_deltas").cloned().and_then(|x| serde_json::from_value::<Vec<RelationDelta>>(x).ok()).unwrap_or_default();
    }
    // Stat deltas fall back to the Reader's feeds when the Psychologist is silent.
    if plan.output.stat_deltas.is_empty() {
        if let Some(f) = plan.reader.get("feeds").and_then(|f| f.as_object()) {
            for (k, v) in f {
                let score = v.as_f64().unwrap_or(0.0).clamp(-2.0, 2.0);
                plan.output.stat_deltas.insert(k.clone(), StatDelta { hp: if score < 0.0 { score * 2.0 } else { 0.0 }, dopamine: score * 5.0, respect_user: score * 2.0, reason: Some("feeds".into()) });
            }
        }
    }
    plan.output.stages = plan.stages.clone();
    // Graph mutations were already applied by the Archivist; hand the rest to the shared path.
    let mut rest = plan.output.clone();
    rest.graph_mutations = GraphMutations::default();
    rest.node_updates.clear();
    crate::director::apply_after_round(app, &rest, &agents, trigger).await?;
    Ok(())
}

/// Apply the Archivist's typed objects, relations, mentions and (at most one) new type.
async fn apply_archivist(app: &AppState, v: &Value, round: &[crate::models::Message], types: &[crate::models::ObjectType]) -> Result<()> {
    let mut type_ids: Vec<String> = types.iter().map(|t| t.id.clone()).collect();
    if let Some(nt) = v.get("new_types").and_then(|x| x.as_array()).and_then(|a| a.first()) {
        if let (Some(id), Some(name)) = (nt.get("id").and_then(|x| x.as_str()), nt.get("name").and_then(|x| x.as_str())) {
            let id = mermaid::safe_id(id);
            if !db::type_exists(&app.db, &id).await? && !id.is_empty() {
                let props = nt.get("props").cloned().unwrap_or(json!([]));
                db::insert_type(&app.db, &id, &mermaid::truncate_chars(name.trim(), 40), nt.get("icon").and_then(|x| x.as_str()).unwrap_or("◇"), "#9a9ca8", &json!({ "props": props }), "archivist").await?;
                type_ids.push(id);
                app.emit(crate::models::RoomEvent::Types { types: db::list_types(&app.db).await? });
            }
        }
    }
    let round_ids: Vec<i64> = round.iter().map(|m| m.id).collect();
    if let Some(objs) = v.get("objects").and_then(|x| x.as_array()) {
        for o in objs.iter().take(5) {
            let Some(title) = o.get("title").and_then(|t| t.as_str()) else { continue };
            let label = mermaid::sanitize_label(title);
            if label == "…" {
                continue;
            }
            let raw_id = o.get("id").and_then(|i| i.as_str()).filter(|i| !i.trim().is_empty()).unwrap_or(title);
            let id = mermaid::safe_id(raw_id);
            let type_id = o.get("type").and_then(|t| t.as_str()).map(|t| t.trim().to_lowercase()).filter(|t| type_ids.contains(t)).unwrap_or_else(|| "note".into());
            let author = mermaid::normalize_author(o.get("author").and_then(|a| a.as_str()).unwrap_or("user"));
            let body = o.get("body").and_then(|b| b.as_str()).map(str::trim).filter(|b| !b.is_empty()).map(|b| mermaid::truncate_chars(b, 1500));
            // keep only schema keys of this type (plus nothing else) so props stay typed
            let schema_keys: Vec<String> = types.iter().find(|t| t.id == type_id).and_then(|t| t.schema.0.get("props")).and_then(|p| p.as_array()).map(|a| a.iter().filter_map(|p| p.get("key").and_then(|k| k.as_str()).map(str::to_string)).collect()).unwrap_or_default();
            let props = o.get("props").and_then(|p| p.as_object()).map(|m| {
                let filtered: serde_json::Map<String, Value> = m.iter().filter(|(k, _)| schema_keys.is_empty() || schema_keys.contains(k)).map(|(k, v)| (k.clone(), v.clone())).collect();
                Value::Object(filtered)
            });
            db::upsert_object(&app.db, &id, &label, &type_id, author, body.as_deref(), props.as_ref(), None, None).await?;
        }
    }
    if let Some(rels) = v.get("relations").and_then(|x| x.as_array()) {
        for r in rels.iter().take(8) {
            let (Some(f), Some(t)) = (r.get("from").and_then(|x| x.as_str()), r.get("to").and_then(|x| x.as_str())) else { continue };
            let from = mermaid::safe_id(f);
            let to = mermaid::safe_id(t);
            if from == to || !db::node_exists(&app.db, &from).await? || !db::node_exists(&app.db, &to).await? {
                continue;
            }
            let kind = r.get("kind").and_then(|k| k.as_str()).map(|k| k.trim().to_lowercase()).filter(|k| ["related", "belongs_to", "about", "blocks", "made_by", "assigned_to", "contradicts", "mentions"].contains(&k.as_str())).unwrap_or_else(|| "related".into());
            let label = r.get("label").and_then(|l| l.as_str()).map(mermaid::sanitize_label).filter(|l| l != "…").map(|l| mermaid::truncate_chars(&l, 30)).unwrap_or_default();
            db::insert_relation(&app.db, &from, &to, &kind, &label, "archivist", 0.8).await?;
        }
    }
    if let Some(ms) = v.get("mentions").and_then(|x| x.as_array()) {
        for m in ms {
            let Some(mid) = m.get("message_id").and_then(|x| x.as_i64()) else { continue };
            if !round_ids.contains(&mid) {
                continue;
            }
            for oid in m.get("objects").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>()).unwrap_or_default() {
                let id = mermaid::safe_id(oid);
                if db::node_exists(&app.db, &id).await? {
                    db::add_mention(&app.db, mid, &id).await?;
                }
            }
        }
    }
    db::place_unpositioned_nodes(&app.db).await?;
    Ok(())
}
