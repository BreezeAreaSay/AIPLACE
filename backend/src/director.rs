//! The LLM Meta-Director: reads the room, invents hidden agendas, picks speakers, applies
//! stat deltas and mutates the knowledge graph. Never panics — a fallback plan is used if
//! the model is unreachable or returns garbage.

use anyhow::Result;
use rand::seq::SliceRandom;

use crate::{
    contracts, db, mermaid,
    models::{Agent, DirectorLogEntry, DirectorOutput, Graph, RoomEvent},
    openrouter::{extract_json, ChatMessage, ChatOpts},
    prompts::{self, DirectorContext},
    state::AppState,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    UserMessage,
    Tick,
    /// A contract was resolved from the UI — the room reacts without a new player line.
    Event,
    /// The player clicked a canvas node: discuss | develop | dispute.
    Node { id: String, mode: String },
    /// Heartbeat: the room acts while the player is away.
    Life(crate::heartbeat::LifeKind),
}

impl Trigger {
    pub fn as_str(&self) -> &'static str {
        match self {
            Trigger::UserMessage => "user_message",
            Trigger::Tick => "tick",
            Trigger::Event => "event",
            Trigger::Node { .. } => "node",
            Trigger::Life(_) => "life",
        }
    }
}

const MAX_BEATS: usize = 4;
const MAX_NODES_PER_ROUND: usize = 6;
const MAX_EDGES_PER_ROUND: usize = 8;

pub async fn run_director(app: &AppState, trigger: &Trigger) -> Result<DirectorLogEntry> {
    let agents = db::list_agents(&app.db).await?;
    let messages = db::recent_messages(&app.db, app.cfg.context_messages).await?;
    let nodes = db::list_nodes(&app.db).await?;
    let edges = db::list_edges(&app.db).await?;
    let open_contracts = db::open_contracts(&app.db).await?;
    let relations = db::list_relations(&app.db).await?;
    let memories = db::list_memories(&app.db).await?;
    let events = db::pending_events(&app.db).await?;
    let now = contracts::now_rfc3339();
    let focus_node = match trigger {
        Trigger::Node { id, .. } => db::get_node(&app.db, id).await?,
        _ => None,
    };
    let focus_notes = match &focus_node {
        Some(n) => db::notes_for_node(&app.db, &n.id).await?,
        None => Vec::new(),
    };
    let focus = match (&focus_node, trigger) {
        (Some(n), Trigger::Node { mode, .. }) => Some((n, focus_notes.as_slice(), mode.as_str())),
        _ => None,
    };
    let recent_speakers = recent_speakers(&messages, &agents);

    let ctx = DirectorContext {
        trigger: trigger.as_str(),
        agents: &agents,
        messages: &messages,
        nodes: &nodes,
        edges: &edges,
        contracts: &open_contracts,
        relations: &relations,
        memories: &memories,
        events: &events,
        now: &now,
        focus,
        recent_speakers: &recent_speakers,
    };
    let msgs = [
        ChatMessage::system(prompts::director_system()),
        ChatMessage::user(prompts::director_user(&ctx)),
    ];
    let opts = ChatOpts {
        temperature: 0.8,
        max_tokens: 2200,
        json_mode: true,
        prefill: None,
        presence_penalty: None,
        frequency_penalty: None,
    };

    let director_model = app.llm_settings().await.director_model;
    let (mut output, source) = match app.llm.chat(&director_model, &msgs, &opts).await {
        Ok(raw) => match extract_json(&raw).and_then(|v| Ok(serde_json::from_value::<DirectorOutput>(v)?)) {
            Ok(o) => (o, "llm"),
            Err(e) => {
                tracing::warn!(error = %e, "director JSON unusable → fallback plan");
                (fallback_plan(&agents, trigger, &recent_speakers), "fallback")
            }
        },
        Err(e) => {
            tracing::warn!(error = %e, "director LLM call failed → fallback plan");
            (fallback_plan(&agents, trigger, &recent_speakers), "fallback")
        }
    };

    normalize_scene(&mut output, &agents, &recent_speakers, false);
    apply_agendas(app, &output, &agents).await?;
    apply_inner_states(app, &output, &agents).await?;
    apply_stats(app, &output, &agents, trigger).await?;
    apply_graph(app, &output).await?;
    apply_relations(app, &output, &agents).await?;
    // Contracts: verdicts first (they may kill someone), then new promises.
    contracts::apply_updates(app, &output.contract_updates).await?;
    let fresh_agents = db::list_agents(&app.db).await?;
    let created = contracts::apply_new(app, &output.new_contracts, &fresh_agents).await?;
    // A freshly appointed guarantor must speak this round.
    for c in &created {
        if let Some(g) = &c.guarantor_id {
            if !output.scene.iter().any(|b| &b.speaker == g) {
                if output.scene.len() >= MAX_BEATS {
                    output.scene.pop();
                }
                output.scene.insert(
                    0,
                    crate::models::Beat {
                        speaker: g.clone(),
                        addressee: Some("user".into()),
                        r#move: Some("предложение".into()),
                        intent: Some(format!("принять ставку по слову #{} «{}» в своей манере и назвать цену провала", c.id, c.title)),
                        delivery: Some("коротко, с нажимом".into()),
                    },
                );
                output.next_speakers = derive_speakers(&output.scene);
            }
        }
    }
    if !created.is_empty() {
        contracts::emit_contracts(app).await?;
    }
    if let Some(last) = events.last() {
        db::mark_events_narrated(&app.db, last.id).await?;
    }

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
    app.emit(RoomEvent::Relations { relations: db::list_relations(&app.db).await? });
    Ok(entry)
}

async fn apply_relations(app: &AppState, out: &DirectorOutput, agents: &[Agent]) -> Result<()> {
    let current = db::list_relations(&app.db).await?;
    for d in out.relation_deltas.iter().take(4) {
        let a = mermaid::normalize_author(&d.a);
        let b = mermaid::normalize_author(&d.b);
        if a == b || a == "user" || b == "user" || !agents.iter().any(|x| x.id == a) || !agents.iter().any(|x| x.id == b) {
            continue;
        }
        let (a, b) = db::order_pair(&a, &b);
        let base = current
            .iter()
            .find(|r| r.a_id == a && r.b_id == b)
            .map(|r| r.affinity)
            .unwrap_or(0);
        let delta = clamp(d.delta.round() as i64, -20, 20);
        if delta == 0 {
            continue;
        }
        let label = d.label.as_deref().map(|l| mermaid::truncate_chars(l.trim(), 80));
        db::upsert_relation(&app.db, a, b, clamp(base + delta, -100, 100), label.as_deref()).await?;
    }
    Ok(())
}

/// Who spoke in the last two rounds (most recent first), for rotation.
pub fn recent_speakers(messages: &[crate::models::Message], agents: &[Agent]) -> Vec<String> {
    let mut seen = Vec::new();
    let mut rounds = 0;
    for m in messages.iter().rev() {
        if m.sender_id == crate::models::USER_ID {
            rounds += 1;
            if rounds >= 2 {
                break;
            }
            continue;
        }
        // chorus one-liners and care notes don't count as "had the floor"
        if matches!(m.r#move.as_deref(), Some("вставка") | Some("забота") | Some("хроника")) {
            continue;
        }
        if agents.iter().any(|a| a.id == m.sender_id) && !seen.contains(&m.sender_id) {
            seen.push(m.sender_id.clone());
        }
    }
    seen
}

/// Safety net: keep the room alive even with no model. Agendas are left untouched (we never
/// hardcode goals) — only a minimal scene is staged, honouring rotation.
fn fallback_plan(agents: &[Agent], trigger: &Trigger, recent: &[String]) -> DirectorOutput {
    let mut alive: Vec<&Agent> = agents.iter().filter(|a| a.is_alive()).collect();
    let mut rng = rand::thread_rng();
    alive.shuffle(&mut rng);
    alive.sort_by_key(|a| recent.iter().position(|r| r == &a.id).map(|p| -(p as i64)).unwrap_or(-100));
    let n = if matches!(trigger, Trigger::UserMessage) { 1 } else { 2 };
    let scene: Vec<crate::models::Beat> = alive
        .iter()
        .take(n)
        .enumerate()
        .map(|(i, a)| crate::models::Beat {
            speaker: a.id.clone(),
            addressee: Some(if i == 0 { "user".to_string() } else { alive[0].id.clone() }),
            r#move: Some("ответ".into()),
            intent: Some("отреагировать на последнюю реплику своими словами".into()),
            delivery: Some("коротко".into()),
        })
        .collect();
    DirectorOutput {
        room_mood: Some("помехи в эфире".into()),
        director_note: Some(
            "Режиссёр не дозвонился до модели: сцена поставлена вслепую, повестки сохранены с прошлого хода.".into(),
        ),
        next_speakers: derive_speakers(&scene),
        scene,
        ..Default::default()
    }
}

pub fn derive_speakers(scene: &[crate::models::Beat]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for b in scene {
        if !out.contains(&b.speaker) {
            out.push(b.speaker.clone());
        }
    }
    out
}

/// Validate the staged scene: known alive speakers, sane addressees, at most MAX_BEATS beats,
/// and a soft rotation rule (someone who spoke two rounds running yields unless addressed).
/// How much each inhabitant has at stake in the player's line (deterministic, cheap).
/// Returns (id, score, reasons) sorted by score desc.
pub fn stakes(agents: &[Agent], last_user: &str, messages: &[crate::models::Message], recent: &[String], own_objects: &[(String, String)]) -> Vec<(String, i32, Vec<&'static str>)> {
    let text = last_user.to_lowercase();
    let words: Vec<&str> = text.split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() >= 3).collect();
    let last_agent = messages.iter().rev().find(|m| m.sender_id != crate::models::USER_ID && m.sender_id != "system").map(|m| m.sender_id.clone());
    let mut out: Vec<(String, i32, Vec<&'static str>)> = agents
        .iter()
        .filter(|a| a.is_alive())
        .map(|a| {
            let mut score = 0;
            let mut why = vec![];
            let name_parts: Vec<String> = a.name.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() >= 3).map(str::to_string).collect();
            if name_parts.iter().any(|n| words.iter().any(|w| w.starts_with(&n[..n.len().min(4)]) && n.starts_with(&w[..w.len().min(4)]))) {
                score += 4;
                why.push("назван по имени");
            }
            let interests: Vec<String> = crate::roles::find(&a.role).map(|p| p.interests.iter().map(|s| s.to_string()).collect()).unwrap_or_else(|| a.archetype.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() >= 5).map(|w| w.chars().take(6).collect()).collect());
            let hits = interests.iter().filter(|i| text.contains(i.as_str())).count();
            if hits > 0 {
                score += (hits as i32).min(3);
                why.push("его тема");
            }
            if own_objects.iter().any(|(owner, label)| owner == &a.id && label.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() >= 4).any(|w| text.contains(w))) {
                score += 2;
                why.push("про его объект");
            }
            if last_agent.as_deref() == Some(a.id.as_str()) {
                score += 1;
                why.push("игрок отвечает ему");
            }
            if recent.first() == Some(&a.id) && recent.iter().filter(|r| *r == &a.id).count() >= 1 && recent.len() >= 2 {
                score -= 2;
                why.push("уже много говорил");
            }
            (a.id.clone(), score, why)
        })
        .collect();
    out.sort_by_key(|x| std::cmp::Reverse(x.1));
    out
}

pub fn stakes_text(st: &[(String, i32, Vec<&'static str>)], agents: &[Agent]) -> String {
    st.iter()
        .map(|(id, score, why)| format!("  {} ({}): {:+}{}", agents.iter().find(|a| &a.id == id).map(|a| a.name.split(' ').next().unwrap_or(&a.name).to_string()).unwrap_or_else(|| id.clone()), id, score, if why.is_empty() { String::new() } else { format!(" — {}", why.join(", ")) }))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Models answer in English or invent moves; the UI and the agents expect the Russian set.
pub fn normalize_move(m: &str) -> String {
    let l = m.trim().to_lowercase();
    let r = match l.as_str() {
        "answer" | "reply" | "response" => "ответ",
        "attack" | "challenge" => "атака",
        "concession" | "concede" | "yield" => "уступка",
        "interruption" | "interrupt" | "interject" => "перебивка",
        "agreement" | "agree" | "support" => "согласие",
        "clarification" | "clarify" | "question" => "уточнение",
        "confession" | "admission" | "admit" => "признание",
        "alliance" | "ally" => "союз",
        "ignore" | "ignore_player" | "dismiss" => "игнор_игрока",
        "silence" | "silent" | "pause" => "молчание",
        "desire" | "wish" | "want" => "желание",
        "proposal" | "suggestion" | "suggest" | "offer" | "propose" => "предложение",
        "objection" | "object" | "counter" | "disagree" | "disagreement" | "возражает" => "возражение",
        "mockery" | "mock" | "tease" | "taunt" => "насмешка",
        "defense" | "defence" | "defend" => "защита",
        other => other,
    };
    mermaid::truncate_chars(r, 30)
}

pub fn normalize_scene(out: &mut DirectorOutput, agents: &[Agent], recent: &[String], work: bool) {
    let alive: Vec<&Agent> = agents.iter().filter(|a| a.is_alive()).collect();
    // Big yards: two full beats, the rest of the room joins as a chorus of one-liners.
    let max_beats = if alive.len() > 3 || work { 2 } else { MAX_BEATS };
    let mut scene: Vec<crate::models::Beat> = Vec::new();
    // Legacy outputs without `scene`: synthesize from next_speakers.
    if out.scene.is_empty() {
        for s in &out.next_speakers {
            scene.push(crate::models::Beat { speaker: s.clone(), addressee: Some("user".into()), ..Default::default() });
        }
    } else {
        scene = out.scene.clone();
    }
    let mut cleaned: Vec<crate::models::Beat> = Vec::new();
    for mut b in scene {
        let id = mermaid::normalize_author(&b.speaker);
        if id == "user" || !alive.iter().any(|a| a.id == id) {
            continue;
        }
        b.speaker = id.to_string();
        b.addressee = Some(match b.addressee.as_deref().map(str::trim) {
            None | Some("") | Some("user") | Some("игрок") | Some("player") => "user".to_string(),
            Some("room") | Some("all") | Some("комната") => "room".to_string(),
            Some(x) => {
                let n = mermaid::normalize_author(x);
                if n == "user" || n == id { "user".to_string() } else { n.to_string() }
            }
        });
        if let Some(m) = &b.r#move {
            let mut mv = normalize_move(m);
            // A working round has no room for theatre: nobody stays silent or mocks the problem.
            if work && !matches!(mv.as_str(), "ответ" | "предложение" | "возражение" | "уточнение" | "согласие" | "признание" | "уступка") {
                mv = "ответ".into();
            }
            b.r#move = Some(mv);
        }
        cleaned.push(b);
        if cleaned.len() == max_beats {
            break;
        }
    }
    // Rotation: an agent who spoke in both of the last two rounds and is not addressed in this
    // scene loses their beat, if someone else remains to speak.
    if recent.len() >= 2 {
        let addressed: Vec<String> = cleaned.iter().filter_map(|b| b.addressee.clone()).collect();
        let tired: Vec<String> = alive
            .iter()
            .map(|a| a.id.clone())
            .filter(|id| recent.iter().filter(|r| *r == id).count() >= 1 && recent.first() == Some(id))
            .collect();
        for t in tired {
            let others_remain = cleaned.iter().any(|b| b.speaker != t);
            if others_remain && !addressed.contains(&t) && cleaned.iter().filter(|b| b.speaker == t).count() == cleaned.len().min(2) {
                cleaned.retain(|b| b.speaker != t);
            }
        }
    }
    if cleaned.is_empty() {
        let mut rng = rand::thread_rng();
        let mut pool: Vec<&&Agent> = alive.iter().filter(|a| !recent.contains(&a.id)).collect();
        if pool.is_empty() {
            pool = alive.iter().collect();
        }
        if let Some(a) = pool.choose(&mut rng) {
            cleaned.push(crate::models::Beat { speaker: a.id.clone(), addressee: Some("user".into()), ..Default::default() });
        }
    }
    out.scene = cleaned;
    out.next_speakers = derive_speakers(&out.scene);
}

async fn apply_inner_states(app: &AppState, out: &DirectorOutput, agents: &[Agent]) -> Result<()> {
    for (raw_id, state) in out.inner_states.iter() {
        let id = mermaid::normalize_author(raw_id);
        let state = state.trim();
        if id == "user" || state.is_empty() || !agents.iter().any(|a| a.id == id) {
            continue;
        }
        db::update_agent_inner_state(&app.db, &id, &mermaid::truncate_chars(state, 200)).await?;
    }
    Ok(())
}

pub async fn apply_agendas_pub(app: &AppState, out: &DirectorOutput, agents: &[Agent]) -> Result<()> {
    apply_agendas(app, out, agents).await
}

/// Post-round bookkeeping shared by both modes: states, stats, graph, relations, events.
pub async fn apply_after_round(app: &AppState, out: &DirectorOutput, agents: &[Agent], trigger: &Trigger) -> Result<()> {
    apply_agendas(app, out, agents).await?;
    apply_inner_states(app, out, agents).await?;
    apply_stats(app, out, agents, trigger).await?;
    apply_graph(app, out).await?;
    apply_relations(app, out, agents).await?;
    app.emit(RoomEvent::Agents { agents: db::list_agents(&app.db).await? });
    let graph = Graph { nodes: db::list_nodes(&app.db).await?, edges: db::list_edges(&app.db).await? };
    let mermaid = mermaid::build_graph(&graph.nodes, &graph.edges);
    app.emit(RoomEvent::Graph { graph, mermaid });
    app.emit(RoomEvent::Relations { relations: db::list_relations(&app.db).await? });
    Ok(())
}

async fn apply_agendas(app: &AppState, out: &DirectorOutput, agents: &[Agent]) -> Result<()> {
    for (raw_id, agenda) in out.updated_agendas.iter() {
        let id = mermaid::normalize_author(raw_id);
        let agenda = agenda.trim();
        if id == "user" || agenda.is_empty() || !agents.iter().any(|a| a.id == id) {
            continue;
        }
        db::update_agent_agenda(&app.db, &id, &mermaid::truncate_chars(agenda, 400)).await?;
    }
    Ok(())
}

fn clamp(v: i64, lo: i64, hi: i64) -> i64 {
    v.max(lo).min(hi)
}

async fn apply_stats(app: &AppState, out: &DirectorOutput, agents: &[Agent], trigger: &Trigger) -> Result<()> {
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
        if matches!(trigger, Trigger::Tick | Trigger::Life(_)) {
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
            db::insert_event(&app.db, "agent_died", &serde_json::json!({ "agent": a.id, "cause": "entropy" })).await?;
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
        let body = n.body.as_deref().map(str::trim).filter(|b| !b.is_empty()).map(|b| mermaid::truncate_chars(b, 1500));
        db::upsert_node_full(
            &app.db,
            &id,
            &label,
            mermaid::normalize_node_type(&n.node_type),
            &mermaid::normalize_author(&n.author),
            body.as_deref(),
            None,
            None,
        )
        .await?;
    }
    for u in out.node_updates.iter().take(6) {
        let id = mermaid::safe_id(&u.id);
        if !db::node_exists(&app.db, &id).await? {
            continue;
        }
        let label = u.label.as_deref().map(mermaid::sanitize_label).filter(|l| l != "…");
        let body = u.body.as_deref().map(str::trim).filter(|b| !b.is_empty()).map(|b| mermaid::truncate_chars(b, 1500));
        let nt = u.node_type.as_deref().map(mermaid::normalize_node_type);
        db::update_node_fields(&app.db, &id, label.as_deref(), body.as_deref(), nt, None).await?;
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
                db::upsert_node(&app.db, id, &label, "concept", &author).await?;
            }
        }
        let label = mermaid::sanitize_label(&e.label);
        let label = if label == "…" { String::new() } else { mermaid::truncate_chars(&label, 30) };
        db::insert_edge(&app.db, &from, &to, &label, &author).await?;
    }
    db::place_unpositioned_nodes(&app.db).await?;
    Ok(())
}

/// After the main beats in a big yard: the bystanders' short reactions, one cheap JSON call.
/// Returns (speaker, addressee, text) lines, posted by the caller.
pub struct ChorusOut {
    pub lines: Vec<(String, String, String)>,
    /// (speaker, emoji, on: "user" | agent id)
    pub reactions: Vec<(String, String, String)>,
}

pub async fn chorus(app: &AppState, agents: &[Agent], round: &[(String, String)], player_line: &str, main_speakers: &[String], stakes: &[(String, i32, Vec<&'static str>)]) -> Result<ChorusOut> {
    let bystanders: Vec<&Agent> = agents.iter().filter(|a| a.is_alive() && !main_speakers.contains(&a.id)).collect();
    if bystanders.is_empty() {
        return Ok(ChorusOut { lines: vec![], reactions: vec![] });
    }
    let cast = bystanders
        .iter()
        .map(|a| {
            let st = stakes.iter().find(|(id, _, _)| id == &a.id).map(|(_, s, _)| *s).unwrap_or(0);
            format!("- {} (id {}): {} | манера: {} | ставка {:+}", a.name, a.id, a.archetype, mermaid::truncate_chars(&a.system_core.chars().skip(a.system_core.find("Говор").unwrap_or(0)).collect::<String>(), 220), st)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let scene = round.iter().map(|(w, t)| format!("[{w}]: {}", mermaid::truncate_chars(t, 400))).collect::<Vec<_>>().join("\n");
    let system = format!("{} Ты — хор двора. Основные реплики уже сказаны; теперь остальные, кто стоял рядом, вставляют по короткой фразе — или молчат. Верни ТОЛЬКО JSON: {{\"lines\": [{{\"speaker\": \"id\", \"addressee\": \"user|room|id\", \"text\": \"до 14 слов\"}}], \"reactions\": [{{\"speaker\": \"id\", \"emoji\": \"один эмодзи\", \"on\": \"user|id основного говорящего\"}}]}}.\nreactions — 0–3 молчаливые реакции эмодзи на реплику игрока или основного говорящего (как в мессенджере): тот, кто не вставил слово, может отреагировать; не ставь реакцию сам себе.\nПравила: 0–2 вставки, каждая — в характере и манере речи этого персонажа и ОБЯЗАТЕЛЬНО добавляет что-то: конкретное наблюдение, факт, приём, точное согласие или несогласие с названным словом, короткую ремарку в *звёздочках*. Запрещены пустые вопросы («а в чём его вина?», «и что же?»), смена темы («давайте о другом»), повтор сказанного и междометия ради присутствия. Сомневаешься — молчи: пустой список лучше шума. Выше ставка — больше шанс вставить; у кого ставка отрицательная — молчит. Без имён в начале фраз. По-русски.", CHORUS_MARK);
    let user = format!("РЕПЛИКА ИГРОКА: «{}»\nОСНОВНЫЕ РЕПЛИКИ РАУНДА:\n{}\nКТО СТОИТ РЯДОМ:\n{}", mermaid::truncate_chars(player_line, 500), scene, cast);
    let model = app.llm_settings().await.director_model;
    let opts = crate::openrouter::ChatOpts { temperature: 0.9, max_tokens: 400, json_mode: true, prefill: None, presence_penalty: Some(0.4), frequency_penalty: Some(0.4) };
    let raw = app.llm.chat(&model, &[crate::openrouter::ChatMessage::system(system), crate::openrouter::ChatMessage::user(user)], &opts).await?;
    let v = crate::openrouter::extract_json(&raw)?;
    let mut out = vec![];
    for l in v.get("lines").and_then(|l| l.as_array()).map(|a| a.iter().take(3).collect::<Vec<_>>()).unwrap_or_default() {
        let sp = mermaid::normalize_author(l.get("speaker").and_then(|x| x.as_str()).unwrap_or(""));
        let text = l.get("text").and_then(|x| x.as_str()).unwrap_or("").trim();
        if text.is_empty() || !bystanders.iter().any(|a| a.id == sp) || out.iter().any(|(s, _, _): &(String, String, String)| s == &sp) {
            continue;
        }
        // A five-word question with nothing in it is the classic weak-model filler: drop it.
        let words = text.split_whitespace().count();
        if (text.ends_with('?') && words <= 6) || text.to_lowercase().contains("поменяем тему") || text.to_lowercase().contains("сменим тему") {
            continue;
        }
        let to = match l.get("addressee").and_then(|x| x.as_str()).map(mermaid::normalize_author).as_deref() {
            Some("user") | None => "user".to_string(),
            Some(x) if x == sp => "room".to_string(),
            Some(x) if agents.iter().any(|a| a.id == x) => x.to_string(),
            _ => "room".to_string(),
        };
        out.push((sp, to, mermaid::truncate_chars(text, 160)));
    }
    let mut reactions = vec![];
    for r in v.get("reactions").and_then(|l| l.as_array()).map(|a| a.iter().take(3).collect::<Vec<_>>()).unwrap_or_default() {
        let sp = mermaid::normalize_author(r.get("speaker").and_then(|x| x.as_str()).unwrap_or(""));
        let emoji: String = r.get("emoji").and_then(|x| x.as_str()).unwrap_or("").trim().chars().take(2).collect();
        let on = mermaid::normalize_author(r.get("on").and_then(|x| x.as_str()).unwrap_or("user"));
        if emoji.is_empty() || !agents.iter().any(|a| a.id == sp && a.is_alive()) || on == sp {
            continue;
        }
        reactions.push((sp, emoji, on));
    }
    Ok(ChorusOut { lines: out, reactions })
}

pub const CHORUS_MARK: &str = "[STAGE-CHORUS]";
