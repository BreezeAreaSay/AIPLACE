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
    let caster_in = prompts::CasterInput {
        trigger: trigger.as_str(),
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
    Ok(RoundPlan { output, reader, stages, first_round_message_id })
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

    let mut log_a = Vec::new();
    let mut log_b = Vec::new();
    let (carto, psych) = tokio::join!(
        call_json(app, "cartographer", prompts::cartographer_system(), prompts::cartographer_user(&round, &agents, &nodes, &edges, focus.as_ref()), 900, &mut log_a),
        call_json(app, "psychologist", prompts::psychologist_system(), prompts::psychologist_user(&round, &agents, &relations, &plan.reader, &events), 900, &mut log_b),
    );
    plan.stages.extend(log_a);
    plan.stages.extend(log_b);

    if let Some(v) = carto {
        plan.output.graph_mutations = GraphMutations {
            nodes: v.get("nodes").cloned().and_then(|x| serde_json::from_value(x).ok()).unwrap_or_default(),
            edges: v.get("edges").cloned().and_then(|x| serde_json::from_value(x).ok()).unwrap_or_default(),
        };
        plan.output.node_updates = v.get("node_updates").cloned().and_then(|x| serde_json::from_value::<Vec<NodeUpdate>>(x).ok()).unwrap_or_default();
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
    crate::director::apply_after_round(app, &plan.output, &agents, trigger).await?;
    Ok(())
}
