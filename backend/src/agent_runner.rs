//! Executes one agent turn: dynamic uncensored system prompt + `<ego_thought>` prefill,
//! then splits the reply into the hidden monologue and the public speech.

use anyhow::{anyhow, Result};
use regex::Regex;
use std::sync::OnceLock;

use crate::{
    db,
    models::{Message, RoomEvent},
    openrouter::{ChatMessage, ChatOpts},
    prompts::{self, AgentContext, EGO_PREFILL},
    state::AppState,
};

/// Everything a turn needs besides the agent id.
#[derive(Default, Clone)]
pub struct TurnPlan<'a> {
    pub beat: Option<&'a crate::models::Beat>,
    pub scene_so_far: Vec<(String, String)>,
    pub room_mood: Option<&'a str>,
    pub focus_node: Option<&'a str>,
    pub library: Option<&'a str>,
}

pub async fn run_agent(app: &AppState, agent_id: &str, plan: TurnPlan<'_>) -> Result<Message> {
    run_agent_inner(app, agent_id, plan, false).await
}

/// Final words of an agent that just hit 0 HP (bypasses the alive check once).
pub async fn run_last_words(app: &AppState, agent_id: &str) -> Result<Message> {
    run_agent_inner(app, agent_id, TurnPlan { room_mood: Some("траур и злорадство"), ..Default::default() }, true).await
}

async fn run_agent_inner(app: &AppState, agent_id: &str, plan: TurnPlan<'_>, dying: bool) -> Result<Message> {
    let agents = db::list_agents(&app.db).await?;
    let agent = agents
        .iter()
        .find(|a| a.id == agent_id)
        .cloned()
        .ok_or_else(|| anyhow!("unknown agent {agent_id}"))?;
    if !agent.is_alive() && !dying {
        return Err(anyhow!("{} is dead and cannot speak", agent.name));
    }
    let others: Vec<_> = agents.iter().filter(|a| a.id != agent_id).cloned().collect();
    let messages = db::recent_messages(&app.db, app.cfg.context_messages).await?;
    let nodes = db::list_nodes(&app.db).await?;
    let own_recent: Vec<String> = messages
        .iter()
        .rev()
        .filter(|m| m.sender_id == agent_id && !m.content.is_empty())
        .take(4)
        .map(|m| m.content.clone())
        .collect();
    let contracts = db::open_contracts(&app.db).await?;
    let relations = db::list_relations(&app.db).await?;
    let memory = db::get_memory(&app.db, agent_id).await?;
    let events = db::recent_events(&app.db, 4).await?;
    let focus_node = match plan.focus_node {
        Some(id) => db::get_node(&app.db, id).await?,
        None => None,
    };
    let focus_notes = match &focus_node {
        Some(n) => db::notes_for_node(&app.db, &n.id).await?,
        None => Vec::new(),
    };
    let focus_files: Option<String> = match &focus_node {
        Some(n) => {
            let atts = db::attachments_of(&app.db, &n.id).await?;
            if atts.is_empty() { None } else { Some(crate::files::digest(&atts, 1800)) }
        }
        None => None,
    };
    let settings = app.llm_settings().await;
    let world = crate::integrations::world_line(app).await;
    let can_search = settings.search_provider().is_some();
    let types = db::list_types(&app.db).await?;
    // Objects the agent can act on: by the topic of the last lines + their own things.
    let topic_text: String = messages.iter().rev().take(4).map(|m| m.content.clone()).collect::<Vec<_>>().join(" ");
    let mut relevant = db::search_objects(&app.db, &topic_text, None, 8).await?;
    for own in db::objects_by_owner(&app.db, agent_id).await?.into_iter().take(4) {
        if !relevant.iter().any(|r| r.id == own.id) {
            relevant.push(own);
        }
    }
    if let Some(n) = &focus_node {
        if !relevant.iter().any(|r| r.id == n.id) {
            relevant.insert(0, n.clone());
        }
    }

    let ctx = AgentContext {
        agent: &agent,
        others: &others,
        messages: &messages,
        nodes: &nodes,
        own_recent: &own_recent,
        room_mood: plan.room_mood,
        contracts: &contracts,
        relations: &relations,
        memory: memory.as_ref().map(|m| m.summary.as_str()),
        events: &events,
        dying,
        beat: plan.beat,
        scene_so_far: &plan.scene_so_far,
        focus: focus_node.as_ref().map(|n| (n, focus_notes.as_slice())),
        library: plan.library,
        focus_files: focus_files.as_deref(),
        world: world.as_deref(),
        can_search,
        types: &types,
        relevant: &relevant,
    };
    let model = if agent.model.trim().is_empty() { settings.model.clone() } else { agent.model.trim().to_string() };
    let (ego, speech) = if app.cfg.agent_mode == "single" {
        let msgs = [
            ChatMessage::system(prompts::agent_system(&ctx)),
            ChatMessage::user(prompts::agent_user(&ctx, &agents)),
        ];
        let opts = ChatOpts {
            temperature: 0.9,
            max_tokens: 700,
            json_mode: false,
            prefill: Some(EGO_PREFILL.to_string()),
            presence_penalty: Some(0.6),
            frequency_penalty: Some(0.4),
        };
        match app.llm.chat(&model, &msgs, &opts).await {
            Ok(raw) => parse_agent_reply(&raw),
            Err(e) => {
                tracing::warn!(agent = %agent_id, error = %e, "agent LLM call failed");
                (Some(format!("(связь с моделью оборвалась: {})", crate::mermaid::truncate_chars(&e.to_string(), 120))), silence_line(&agent.id))
            }
        }
    } else {
        think_then_speak(app, &model, &ctx, &agents).await
    };
    let speech = strip_name_prefix(&speech, &agent.name);
    let (speech, acts) = extract_acts(&speech);
    let (speech, notes) = extract_notes(&speech);
    let speech = if speech.trim().is_empty() { "*молчит*".to_string() } else { speech };
    let addressee = plan.beat.and_then(|b| b.addressee.clone());
    let mv = plan.beat.and_then(|b| b.r#move.clone());
    let msg = db::insert_message_full(
        &app.db,
        agent_id,
        ego.as_deref(),
        &speech,
        addressee.as_deref(),
        mv.as_deref(),
        plan.focus_node,
    )
    .await?;
    app.emit(RoomEvent::Message { message: msg.clone() });
    // Library actions the agent took with this line.
    let records = apply_acts(app, agent_id, &acts, &types).await;
    let msg = if records.is_empty() {
        msg
    } else {
        db::set_message_actions(&app.db, msg.id, &records).await?;
        for r in &records {
            db::add_mention(&app.db, msg.id, &r.object_id).await?;
        }
        let graph = crate::models::Graph { nodes: db::list_nodes(&app.db).await?, edges: db::list_edges(&app.db).await? };
        let mermaid = crate::mermaid::build_graph(&graph.nodes, &graph.edges);
        app.emit(RoomEvent::Graph { graph, mermaid });
        let updated = db::with_mentions(&app.db, vec![db::recent_messages(&app.db, 1).await?.remove(0)]).await?.remove(0);
        app.emit(RoomEvent::Message { message: updated.clone() });
        updated
    };
    let mut stored = 0;
    for (node_id, text) in notes {
        let id = crate::mermaid::safe_id(&node_id);
        if db::node_exists(&app.db, &id).await? {
            db::insert_note(&app.db, &id, agent_id, &crate::mermaid::truncate_chars(text.trim(), 600)).await?;
            stored += 1;
        }
    }
    if stored > 0 {
        app.emit(RoomEvent::Notes { notes: db::list_notes(&app.db, 400).await? });
    }
    Ok(msg)
}

/// Pipeline turn: (1) a short private thought + the phrase to latch onto, (2) the line itself,
/// (3) when a node is in focus, a one-line note on it. Each call is small enough for weak models.
async fn think_then_speak(app: &AppState, model: &str, ctx: &AgentContext<'_>, agents: &[crate::models::Agent]) -> (Option<String>, String) {
    let think_opts = ChatOpts { temperature: 0.8, max_tokens: 220, json_mode: false, prefill: None, presence_penalty: Some(0.3), frequency_penalty: Some(0.3) };
    let speak_opts = ChatOpts { temperature: 0.9, max_tokens: 400, json_mode: false, prefill: None, presence_penalty: Some(0.6), frequency_penalty: Some(0.5) };

    let thought_raw = match app
        .llm
        .chat(model, &[ChatMessage::system(prompts::think_system(ctx)), ChatMessage::user(prompts::think_user(ctx, agents))], &think_opts)
        .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(agent = %ctx.agent.id, error = %e, "think stage failed");
            String::new()
        }
    };
    let (ego, hook, query, web) = prompts::split_thought(&thought_raw);
    let mut found_parts: Vec<String> = vec![];
    if let Some(q) = query {
        let hits = db::search_objects(&app.db, &q, None, 6).await.unwrap_or_default();
        let types = db::list_types(&app.db).await.unwrap_or_default();
        found_parts.push(format!("В БИБЛИОТЕКЕ по «{q}»:\n{}", prompts::library_digest(&hits, &types, agents)));
    }
    if let Some(q) = web.filter(|_| ctx.can_search) {
        app.emit(RoomEvent::Status { stage: "agent".into(), detail: format!("{} ищет в сети: {}", ctx.agent.name, crate::mermaid::truncate_chars(&q, 40)) });
        let s = app.llm_settings().await;
        match crate::integrations::search::search(&s, &q, 5).await {
            Ok(hits) => found_parts.push(format!("В СЕТИ по «{q}»:\n{}", crate::integrations::search::digest(&hits))),
            Err(e) => tracing::warn!(error = %e, "web search failed"),
        }
    }
    let found: Option<String> = if found_parts.is_empty() { None } else { Some(found_parts.join("\n")) };

    // Native tool calling when the model supports it (or forced): the agent *uses* the system.
    let native = match app.cfg.agent_tools.as_str() {
        "native" => true,
        "tags" => false,
        _ => {
            let key = app.llm_settings().await.api_key;
            key.is_some() && app.catalog.supports_tools(key.as_deref(), model).await
        }
    };
    if native {
        return speak_with_tools(app, model, ctx, agents, &ego, &hook, found.as_deref()).await;
    }
    app.emit(RoomEvent::StreamStart { agent: ctx.agent.id.clone() });
    let agent_id = ctx.agent.id.clone();
    let emitter = app.clone();
    let speech = match app
        .llm
        .chat_stream(
            model,
            &[ChatMessage::system(prompts::speak_system(ctx)), ChatMessage::user(prompts::speak_user(ctx, agents, &ego, &hook, found.as_deref()))],
            &speak_opts,
            |d| emitter.emit(RoomEvent::Delta { agent: agent_id.clone(), text: d.to_string() }),
        )
        .await
    {
        Ok(t) => t.trim().trim_matches('"').to_string(),
        Err(e) => {
            tracing::warn!(agent = %ctx.agent.id, error = %e, "speak stage failed");
            silence_line(&ctx.agent.id)
        }
    };
    let speech = strip_labels(&speech);

    let mut speech = speech;
    if let Some((node, _)) = ctx.focus {
        let note_opts = ChatOpts { temperature: 0.7, max_tokens: 120, json_mode: false, prefill: None, presence_penalty: None, frequency_penalty: None };
        if let Ok(n) = app
            .llm
            .chat(model, &[ChatMessage::system(prompts::note_system(ctx)), ChatMessage::user(prompts::note_user(ctx, &speech))], &note_opts)
            .await
        {
            let n = n.trim().trim_matches('"').trim();
            if !n.is_empty() && !n.eq_ignore_ascii_case("нет") {
                speech.push_str(&format!("\n<note node=\"{}\">{}</note>", node.id, crate::mermaid::truncate_chars(n, 400)));
            }
        }
    }
    (if ego.is_empty() { None } else { Some(ego) }, speech)
}

/// Tools-enabled Speak: the model may call library functions (search before answering,
/// create/update/link/note as acts); results come back as ActionRecords appended as <act>-free
/// records via a side channel (we encode them into the speech tail for the common path).
async fn speak_with_tools(app: &AppState, model: &str, ctx: &AgentContext<'_>, agents: &[crate::models::Agent], ego: &str, hook: &str, found: Option<&str>) -> (Option<String>, String) {
    use crate::openrouter::ToolTurn;
    let opts = ChatOpts { temperature: 0.9, max_tokens: 500, json_mode: false, prefill: None, presence_penalty: Some(0.6), frequency_penalty: Some(0.5) };
    let tools = crate::tools::as_openai_tools(&app.llm_settings().await);
    let mut history: Vec<serde_json::Value> = vec![
        serde_json::json!({ "role": "system", "content": prompts::speak_system_native(ctx) }),
        serde_json::json!({ "role": "user", "content": prompts::speak_user(ctx, agents, ego, hook, found) }),
    ];
    let mut records: Vec<crate::models::ActionRecord> = Vec::new();
    let mut text = String::new();
    for round in 0..4 {
        match app.llm.chat_tools_step(model, &history, &tools, &opts).await {
            Ok(ToolTurn::Text(t)) => {
                text = t;
                break;
            }
            Ok(ToolTurn::Calls { assistant_message, calls }) => {
                history.push(assistant_message);
                for c in calls.iter().take(4) {
                    app.emit(RoomEvent::Status { stage: "agent".into(), detail: format!("{} → {}", ctx.agent.name, c.name) });
                    let (result, rec) = match crate::tools::call(app, &ctx.agent.id, &c.name, &c.args).await {
                        Ok(r) => r,
                        Err(e) => (serde_json::json!({ "error": e.to_string() }), None),
                    };
                    if let Some(r) = rec {
                        records.push(r);
                    }
                    history.push(serde_json::json!({ "role": "tool", "tool_call_id": c.id, "name": c.name, "content": result.to_string() }));
                }
                if round == 3 {
                    // Out of rounds: ask for the line without tools.
                    if let Ok(t) = app.llm.chat(model, &[ChatMessage::system(prompts::speak_system_native(ctx)), ChatMessage::user(format!("{}\n\n(Инструменты уже использованы. Теперь только реплика.)", prompts::speak_user(ctx, agents, ego, hook, found)))], &opts).await {
                        text = t;
                    }
                }
            }
            Err(e) => {
                tracing::warn!(agent = %ctx.agent.id, error = %e, "tools step failed → tag mode for this line");
                if let Ok(t) = app.llm.chat(model, &[ChatMessage::system(prompts::speak_system(ctx)), ChatMessage::user(prompts::speak_user(ctx, agents, ego, hook, found))], &opts).await {
                    text = t;
                }
                break;
            }
        }
    }
    let speech = strip_labels(text.trim().trim_matches('"'));
    // Carry the tool records through the common path as synthetic <act> markers.
    let mut tail = String::new();
    for r in records {
        tail.push_str(&format!("\n<act>{}</act>", serde_json::json!({ "op": "record", "object_id": r.object_id, "text": r.text, "kind": r.op })));
    }
    app.emit(RoomEvent::StreamStart { agent: ctx.agent.id.clone() });
    app.emit(RoomEvent::Delta { agent: ctx.agent.id.clone(), text: speech.clone() });
    (if ego.is_empty() { None } else { Some(ego.to_string()) }, format!("{speech}{tail}"))
}

fn strip_labels(s: &str) -> String {
    // Some models still emit the two-layer format: keep only the public part.
    let s = match s.find("</ego_thought>") {
        Some(i) => &s[i + "</ego_thought>".len()..],
        None => s,
    };
    let s = s.trim_start_matches(EGO_PREFILL);
    static LABELS: OnceLock<Regex> = OnceLock::new();
    let re = LABELS.get_or_init(|| Regex::new(r"(?im)^\s*(\*\*)?\s*(реплика|речь|публичная речь|ответ)\s*:?\s*(\*\*)?\s*:?\s*").expect("regex"));
    re.replace_all(s, "").trim().to_string()
}

/// Pull `<act>{json}</act>` blocks out of the speech.
pub fn extract_acts(speech: &str) -> (String, Vec<serde_json::Value>) {
    static ACT: OnceLock<Regex> = OnceLock::new();
    let re = ACT.get_or_init(|| Regex::new(r"(?is)<act>\s*(\{.*?\})\s*</act>").expect("regex"));
    let mut acts = Vec::new();
    for c in re.captures_iter(speech) {
        if let Ok(v) = crate::openrouter::extract_json(&c[1]) {
            acts.push(v);
        }
    }
    let cleaned = re.replace_all(speech, "").trim().to_string();
    (cleaned, acts)
}

/// Apply agent actions against the library; returns human-readable records for the UI.
async fn apply_acts(app: &AppState, agent_id: &str, acts: &[serde_json::Value], types: &[crate::models::ObjectType]) -> Vec<crate::models::ActionRecord> {
    let mut out = Vec::new();
    for a in acts.iter().take(3) {
        let op = a.get("op").and_then(|o| o.as_str()).unwrap_or("").to_lowercase();
        let res: anyhow::Result<Option<crate::models::ActionRecord>> = async {
            match op.as_str() {
                "record" => Ok(Some(crate::models::ActionRecord {
                    op: a.get("kind").and_then(|k| k.as_str()).unwrap_or("update").to_string(),
                    object_id: a.get("object_id").and_then(|k| k.as_str()).unwrap_or("").to_string(),
                    text: a.get("text").and_then(|k| k.as_str()).unwrap_or("").to_string(),
                })),
                "create" => {
                    let title = a.get("title").and_then(|t| t.as_str()).map(crate::mermaid::sanitize_label).filter(|t| t != "…");
                    let Some(title) = title else { return Ok(None) };
                    let type_id = a.get("type").and_then(|t| t.as_str()).map(|t| t.trim().to_lowercase()).filter(|t| types.iter().any(|x| &x.id == t)).unwrap_or_else(|| "note".into());
                    let mut id = crate::mermaid::safe_id(&title);
                    if db::node_exists(&app.db, &id).await? {
                        id = format!("{}_{}", id, chrono::Utc::now().timestamp() % 10_000);
                    }
                    let body = a.get("body").and_then(|b| b.as_str()).map(|b| crate::mermaid::truncate_chars(b.trim(), 1500));
                    let keys: Vec<String> = types.iter().find(|t| t.id == type_id).and_then(|t| t.schema.0.get("props")).and_then(|p| p.as_array()).map(|arr| arr.iter().filter_map(|p| p.get("key").and_then(|k| k.as_str()).map(str::to_string)).collect()).unwrap_or_default();
                    let props = a.get("props").and_then(|p| p.as_object()).map(|m| serde_json::Value::Object(m.iter().filter(|(k, _)| keys.is_empty() || keys.contains(k)).map(|(k, v)| (k.clone(), v.clone())).collect()));
                    db::upsert_object(&app.db, &id, &title, &type_id, agent_id, body.as_deref(), props.as_ref(), None, None).await?;
                    db::place_unpositioned_nodes(&app.db).await?;
                    let tname = crate::tools::type_name_acc(types, &type_id);
                    Ok(Some(crate::models::ActionRecord { op: "create".into(), object_id: id, text: format!("завёл {tname} «{title}»") }))
                }
                "append" => {
                    let (_, rec) = crate::tools::call(app, agent_id, "library_append", a).await?;
                    Ok(rec)
                }
                "update" | "done" => {
                    let Some(id) = a.get("id").and_then(|i| i.as_str()).map(crate::mermaid::safe_id) else { return Ok(None) };
                    let Some(existing) = db::get_node(&app.db, &id).await? else { return Ok(None) };
                    let label = a.get("title").and_then(|t| t.as_str()).map(crate::mermaid::sanitize_label).filter(|t| t != "…");
                    let body = a.get("body").and_then(|b| b.as_str()).map(|b| crate::mermaid::truncate_chars(b.trim(), 1500));
                    let type_id = a.get("type").and_then(|t| t.as_str()).map(|t| t.trim().to_lowercase()).filter(|t| types.iter().any(|x| &x.id == t));
                    let mut props = a.get("props").cloned();
                    if op == "done" {
                        props = Some(serde_json::json!({ "done": true, "status": "сделана" }));
                    }
                    if label.is_some() || body.is_some() {
                        db::update_node_fields(&app.db, &id, label.as_deref(), body.as_deref(), None, None).await?;
                    }
                    if type_id.is_some() || props.is_some() {
                        db::update_object_meta(&app.db, &id, type_id.as_deref(), props.as_ref(), None).await?;
                    }
                    let what = if props.as_ref().and_then(|p| p.get("done")).and_then(|d| d.as_bool()) == Some(true) {
                        format!("закрыл «{}»", existing.label)
                    } else if let Some(p) = props.as_ref().and_then(|p| p.as_object()).filter(|o| !o.is_empty()) {
                        format!("обновил «{}»: {}", existing.label, p.iter().map(|(k, v)| format!("{k}={}", v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))).collect::<Vec<_>>().join(", "))
                    } else {
                        format!("поправил «{}»", existing.label)
                    };
                    Ok(Some(crate::models::ActionRecord { op: "update".into(), object_id: id, text: what }))
                }
                "link" => {
                    let (Some(f), Some(t)) = (a.get("from").and_then(|x| x.as_str()), a.get("to").and_then(|x| x.as_str())) else { return Ok(None) };
                    let from = crate::mermaid::safe_id(f);
                    let to = crate::mermaid::safe_id(t);
                    if from == to || !db::node_exists(&app.db, &from).await? || !db::node_exists(&app.db, &to).await? {
                        return Ok(None);
                    }
                    let kind = a.get("kind").and_then(|k| k.as_str()).map(|k| k.trim().to_lowercase()).filter(|k| ["related", "belongs_to", "about", "blocks", "made_by", "assigned_to", "contradicts", "mentions"].contains(&k.as_str())).unwrap_or_else(|| "related".into());
                    let label = a.get("label").and_then(|l| l.as_str()).map(crate::mermaid::sanitize_label).filter(|l| l != "…").map(|l| crate::mermaid::truncate_chars(&l, 30)).unwrap_or_default();
                    db::insert_relation(&app.db, &from, &to, &kind, &label, agent_id, 0.9).await?;
                    let fl = db::get_node(&app.db, &from).await?.map(|n| n.label).unwrap_or(from.clone());
                    let tl = db::get_node(&app.db, &to).await?.map(|n| n.label).unwrap_or(to.clone());
                    Ok(Some(crate::models::ActionRecord { op: "link".into(), object_id: from, text: format!("связал «{fl}» → «{tl}»") }))
                }
                _ => Ok(None),
            }
        }
        .await;
        match res {
            Ok(Some(r)) => out.push(r),
            Ok(None) => {}
            Err(e) => tracing::warn!(agent = %agent_id, error = %e, "act failed"),
        }
    }
    out
}

/// Pull `<note node="id">text</note>` annotations out of the speech.
pub fn extract_notes(speech: &str) -> (String, Vec<(String, String)>) {
    static NOTE: OnceLock<Regex> = OnceLock::new();
    let re = NOTE.get_or_init(|| Regex::new(r#"(?is)<note\s+node\s*=\s*["']?([^"'>\s]+)["']?\s*>(.*?)</note>"#).expect("regex"));
    let mut notes = Vec::new();
    for c in re.captures_iter(speech) {
        let text = c[2].trim();
        if !text.is_empty() {
            notes.push((c[1].to_string(), text.to_string()));
        }
    }
    let cleaned = re.replace_all(speech, "").trim().to_string();
    (cleaned, notes)
}

fn silence_line(agent_id: &str) -> String {
    match agent_id {
        "physicist" => "…(Аркадий Борисович хмуро смотрит на мигающую лампу и молчит. Помехи.)".into(),
        "lit_teacher" => "…(Софья Генриховна поджимает губы: связь с комнатой прервалась на самом интересном месте.)".into(),
        "graffiti" => "…(Кэп стучит по рации: «алло, база, палево, сигнал пропал».)".into(),
        _ => "…(молчит: связь с комнатой на секунду пропала)".into(),
    }
}

/// Split `<ego_thought>…</ego_thought>\n\nspeech`. Tolerates a missing closing tag and
/// models that label the layers ("Публичная речь:").
pub fn parse_agent_reply(raw: &str) -> (Option<String>, String) {
    static LABELS: OnceLock<Regex> = OnceLock::new();
    let labels = LABELS.get_or_init(|| {
        Regex::new(r"(?im)^\s*(\*\*)?\s*(публичная речь|public_speech|public speech|речь|реплика)\s*:?\s*(\*\*)?\s*:?\s*").expect("regex")
    });
    let text = raw.trim();
    let text = if text.starts_with(EGO_PREFILL) { text.to_string() } else { format!("{EGO_PREFILL}{text}") };
    let body = &text[EGO_PREFILL.len()..];

    let (ego, speech) = if let Some(idx) = body.find("</ego_thought>") {
        let ego = body[..idx].trim().to_string();
        let speech = body[idx + "</ego_thought>".len()..].trim().to_string();
        (ego, speech)
    } else if let Some(idx) = body.find("\n\n") {
        // No closing tag: first paragraph is the thought, the rest is speech.
        (body[..idx].trim().to_string(), body[idx..].trim().to_string())
    } else {
        (String::new(), body.trim().to_string())
    };
    let speech = labels.replace_all(&speech, "").trim().to_string();
    let speech = speech.trim_matches('"').trim().to_string();
    let ego = ego.trim_matches('"').trim().to_string();
    if speech.is_empty() {
        // Model put everything in the thought — surface it so the room isn't silent.
        return (None, if ego.is_empty() { "…".into() } else { ego });
    }
    (if ego.is_empty() { None } else { Some(ego) }, speech)
}

fn strip_name_prefix(speech: &str, name: &str) -> String {
    let short = name.split(['(', ' ']).next().unwrap_or(name);
    let mut s = speech.to_string();
    for prefix in [format!("{name}:"), format!("{short}:"), format!("[{name}]:"), format!("[{short}]:")] {
        if let Some(rest) = s.strip_prefix(&prefix) {
            s = rest.trim().to_string();
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_two_layers() {
        let (ego, speech) = parse_agent_reply("<ego_thought>злюсь</ego_thought>\n\nНу и? Что дальше?");
        assert_eq!(ego.as_deref(), Some("злюсь"));
        assert_eq!(speech, "Ну и? Что дальше?");
    }

    #[test]
    fn extracts_node_notes() {
        let (speech, notes) = extract_notes("Полка — это масса на кронштейне.\n<note node=\"polka\">Считать нагрузку: 12 кг/м.</note>");
        assert_eq!(speech, "Полка — это масса на кронштейне.");
        assert_eq!(notes, vec![("polka".to_string(), "Считать нагрузку: 12 кг/м.".to_string())]);
    }

    #[test]
    fn handles_missing_prefill_and_label() {
        let (ego, speech) = parse_agent_reply("мысль</ego_thought>\n**Публичная речь:** Говорю.");
        assert_eq!(ego.as_deref(), Some("мысль"));
        assert_eq!(speech, "Говорю.");
    }
}
