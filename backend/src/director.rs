//! The LLM Meta-Director: reads the room, invents hidden agendas, picks speakers, applies
//! stat deltas and mutates the knowledge graph. Never panics — a fallback plan is used if
//! the model is unreachable or returns garbage.

use anyhow::Result;
use rand::seq::SliceRandom;

use crate::{
    db, mermaid,
    models::{Agent, DirectorLogEntry, DirectorOutput, Graph, RoomEvent},
    openrouter::{extract_json, ChatMessage, ChatOpts},
    prompts::{self, DirectorContext},
    state::AppState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    UserMessage,
    Tick,
}

impl Trigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Trigger::UserMessage => "user_message",
            Trigger::Tick => "tick",
        }
    }
}

const MAX_SPEAKERS: usize = 2;
const MAX_NODES_PER_ROUND: usize = 6;
const MAX_EDGES_PER_ROUND: usize = 8;

pub async fn run_director(app: &AppState, trigger: Trigger) -> Result<DirectorLogEntry> {
    let agents = db::list_agents(&app.db).await?;
    let messages = db::recent_messages(&app.db, app.cfg.context_messages).await?;
    let nodes = db::list_nodes(&app.db).await?;
    let edges = db::list_edges(&app.db).await?;

    let ctx = DirectorContext {
        trigger: trigger.as_str(),
        agents: &agents,
        messages: &messages,
        nodes: &nodes,
        edges: &edges,
    };
    let msgs = [
        ChatMessage::system(prompts::director_system()),
        ChatMessage::user(prompts::director_user(&ctx)),
    ];
    let opts = ChatOpts {
        temperature: 0.85,
        max_tokens: 1800,
        json_mode: true,
        prefill: None,
    };

    let director_model = app.llm_settings().await.director_model;
    let (mut output, source) = match app.llm.chat(&director_model, &msgs, &opts).await {
        Ok(raw) => match extract_json(&raw).and_then(|v| Ok(serde_json::from_value::<DirectorOutput>(v)?)) {
            Ok(o) => (o, "llm"),
            Err(e) => {
                tracing::warn!(error = %e, "director JSON unusable → fallback plan");
                (fallback_plan(&agents, trigger), "fallback")
            }
        },
        Err(e) => {
            tracing::warn!(error = %e, "director LLM call failed → fallback plan");
            (fallback_plan(&agents, trigger), "fallback")
        }
    };

    normalize_speakers(&mut output, &agents);
    apply_agendas(app, &output, &agents).await?;
    apply_stats(app, &output, &agents, trigger).await?;
    apply_graph(app, &output).await?;

    let entry = db::insert_director_log(&app.db, trigger.as_str(), source, &output).await?;
    app.emit(RoomEvent::Director { entry: entry.clone() });
    let agents = db::list_agents(&app.db).await?;
    app.emit(RoomEvent::Agents { agents });
    let graph = Graph {
        nodes: db::list_nodes(&app.db).await?,
        edges: db::list_edges(&app.db).await?,
    };
    let mermaid = mermaid::build_graph(&graph.nodes, &graph.edges);
    app.emit(RoomEvent::Graph { graph, mermaid });
    Ok(entry)
}

/// Safety net: keep the room alive even with no model. Agendas are left untouched (we never
/// hardcode goals) — only speakers are chosen.
fn fallback_plan(agents: &[Agent], trigger: Trigger) -> DirectorOutput {
    let mut alive: Vec<&Agent> = agents.iter().filter(|a| a.is_alive()).collect();
    let mut rng = rand::thread_rng();
    alive.shuffle(&mut rng);
    let n = if trigger == Trigger::Tick { 2 } else { 1 };
    DirectorOutput {
        room_mood: Some("помехи в эфире".into()),
        director_note: Some(
            "Режиссёр не дозвонился до модели: говорящие выбраны случайно, повестки сохранены с прошлого хода.".into(),
        ),
        next_speakers: alive.iter().take(n).map(|a| a.id.clone()).collect(),
        ..Default::default()
    }
}

fn normalize_speakers(out: &mut DirectorOutput, agents: &[Agent]) {
    let alive: Vec<&Agent> = agents.iter().filter(|a| a.is_alive()).collect();
    let mut speakers: Vec<String> = Vec::new();
    for s in out.next_speakers.iter() {
        let id = mermaid::normalize_author(s);
        if id != "user" && alive.iter().any(|a| a.id == id) && !speakers.iter().any(|x| x == id) {
            speakers.push(id.to_string());
        }
        if speakers.len() == MAX_SPEAKERS {
            break;
        }
    }
    if speakers.is_empty() {
        let mut rng = rand::thread_rng();
        if let Some(a) = alive.choose(&mut rng) {
            speakers.push(a.id.clone());
        }
    }
    out.next_speakers = speakers;
}

async fn apply_agendas(app: &AppState, out: &DirectorOutput, agents: &[Agent]) -> Result<()> {
    for (raw_id, agenda) in out.updated_agendas.iter() {
        let id = mermaid::normalize_author(raw_id);
        let agenda = agenda.trim();
        if id == "user" || agenda.is_empty() || !agents.iter().any(|a| a.id == id) {
            continue;
        }
        db::update_agent_agenda(&app.db, id, &mermaid::truncate_chars(agenda, 400)).await?;
    }
    Ok(())
}

fn clamp(v: i64, lo: i64, hi: i64) -> i64 {
    v.max(lo).min(hi)
}

async fn apply_stats(app: &AppState, out: &DirectorOutput, agents: &[Agent], trigger: Trigger) -> Result<()> {
    for a in agents.iter().filter(|a| a.is_alive()) {
        let d = out
            .stat_deltas
            .iter()
            .find(|(k, _)| mermaid::normalize_author(k) == a.id)
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        // The director's deltas are bounded so one wild JSON can't kill a character outright.
        let mut hp = a.hp + clamp(d.hp.round() as i64, -12, 12);
        let mut dopamine = a.dopamine + clamp(d.dopamine.round() as i64, -20, 20);
        let respect = clamp(a.respect_user + clamp(d.respect_user.round() as i64, -15, 15), -100, 100);
        if trigger == Trigger::Tick {
            // Entropy: time alone without being fed costs something.
            hp -= app.cfg.tick_entropy_hp;
            dopamine -= app.cfg.tick_entropy_dopamine;
        }
        let hp = clamp(hp, 0, 100);
        let dopamine = clamp(dopamine, 0, 100);
        db::update_agent_stats(&app.db, &a.id, hp, dopamine, respect).await?;
        if hp == 0 {
            tracing::warn!(agent = %a.id, "PERMADEATH: agent reached 0 HP");
            db::insert_message(
                &app.db,
                "system",
                None,
                &format!("☠ {} больше не отвечает. HP = 0. Комната стала тише.", a.name),
            )
            .await?;
        }
    }
    Ok(())
}

async fn apply_graph(app: &AppState, out: &DirectorOutput) -> Result<()> {
    let m = &out.graph_mutations;
    for n in m.nodes.iter().take(MAX_NODES_PER_ROUND) {
        let label = mermaid::sanitize_label(&n.label);
        if label == "…" {
            continue;
        }
        let raw_id = if n.id.trim().is_empty() { n.label.as_str() } else { n.id.as_str() };
        let id = mermaid::safe_id(raw_id);
        db::upsert_node(
            &app.db,
            &id,
            &label,
            mermaid::normalize_node_type(&n.node_type),
            mermaid::normalize_author(&n.author),
        )
        .await?;
    }
    for e in m.edges.iter().take(MAX_EDGES_PER_ROUND) {
        let from = mermaid::safe_id(&e.from);
        let to = mermaid::safe_id(&e.to);
        if from == to {
            continue;
        }
        let author = mermaid::normalize_author(e.author.as_deref().unwrap_or("user"));
        for (id, raw) in [(&from, &e.from), (&to, &e.to)] {
            if !db::node_exists(&app.db, id).await? {
                // The director referenced a node it forgot to declare — materialize it.
                let label = mermaid::sanitize_label(&raw.replace('_', " "));
                db::upsert_node(&app.db, id, &label, "concept", author).await?;
            }
        }
        let label = mermaid::sanitize_label(&e.label);
        let label = if label == "…" { String::new() } else { mermaid::truncate_chars(&label, 30) };
        db::insert_edge(&app.db, &from, &to, &label, author).await?;
    }
    Ok(())
}
