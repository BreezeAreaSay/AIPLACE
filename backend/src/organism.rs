//! The library as a living thing. Inhabitants don't just answer — they *tend* the library:
//! digest files you drop, link orphans, close the loop on stale tasks, write the day's
//! chronicle, advance their own projects. Each act is one small LLM call producing a JSON
//! plan that is applied through the same tool registry the agents use, so it shows up in the
//! chat as that character's doing.

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{json, Value};

use crate::{
    db, mermaid,
    models::{ActionRecord, Agent, Attachment, GraphNode, RoomEvent},
    openrouter::{extract_json, ChatMessage, ChatOpts},
    state::AppState,
};

pub const GARDENER_MARK: &str = "[STAGE-GARDENER]";
pub const CHRONICLE_MARK: &str = "[STAGE-CHRONICLE]";

#[derive(Debug, Clone, Serialize)]
pub struct Health {
    pub objects: usize,
    pub archived: usize,
    pub orphans: usize,
    pub stale: usize,
    pub undigested_files: usize,
    pub open_tasks: usize,
    pub overdue_tasks: usize,
    pub unread_bookmarks: usize,
    pub grown_7d: usize,
    pub touched_by_agents_7d: usize,
    /// 0..100 — how cared-for the library is.
    pub score: u8,
    pub today_diary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Care {
    pub kind: String,
    pub node_id: String,
    pub reason: String,
}

fn days_since(ts: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(ts)
        .map(|d| d.with_timezone(&chrono::Utc))
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.fZ").map(|n| n.and_utc()))
        .map(|d| (chrono::Utc::now() - d).num_days())
        .unwrap_or(0)
}

pub fn today_diary_id() -> String {
    format!("khronika_{}", chrono::Local::now().format("%Y_%m_%d"))
}

pub async fn health(app: &AppState) -> Result<Health> {
    let nodes = db::list_nodes(&app.db).await?;
    let edges = db::list_edges(&app.db).await?;
    let atts = db::list_attachments(&app.db).await?;
    let mentioned: std::collections::HashSet<String> = db::all_mentioned_objects(&app.db).await?.into_iter().collect();
    let linked: std::collections::HashSet<&str> = edges.iter().flat_map(|e| [e.source_id.as_str(), e.target_id.as_str()]).collect();
    let live: Vec<&GraphNode> = nodes.iter().filter(|n| n.archived == 0).collect();
    let orphans = live.iter().filter(|n| !linked.contains(n.id.as_str()) && !mentioned.contains(&n.id) && n.type_id != "feed").count();
    let stale = live
        .iter()
        .filter(|n| days_since(&n.updated_at) > 30 && !(n.type_id == "task" && n.props.0.get("done").and_then(|d| d.as_bool()) == Some(true)) && !matches!(n.type_id.as_str(), "bookmark" | "quote" | "image" | "file"))
        .count();
    let undigested_files = atts.iter().filter(|a| a.text_len > 0 && !is_digested(&nodes, a)).count();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let (mut open_tasks, mut overdue) = (0, 0);
    for n in live.iter().filter(|n| n.type_id == "task") {
        let done = n.props.0.get("done").and_then(|d| d.as_bool()) == Some(true);
        if done {
            continue;
        }
        open_tasks += 1;
        if n.props.0.get("due").and_then(|d| d.as_str()).map(|d| d < today.as_str()).unwrap_or(false) {
            overdue += 1;
        }
    }
    let unread_bookmarks = live.iter().filter(|n| n.type_id == "bookmark" && n.props.0.get("read").and_then(|r| r.as_bool()) != Some(true)).count();
    let grown_7d = nodes.iter().filter(|n| days_since(&n.created_at) <= 7).count();
    let touched_by_agents_7d = nodes.iter().filter(|n| days_since(&n.updated_at) <= 7 && n.created_by != "user").count();
    let total = live.len().max(1) as f64;
    let penalty = (orphans as f64 / total) * 35.0 + (stale as f64 / total) * 25.0 + (undigested_files.min(10) as f64) * 4.0 + (overdue.min(10) as f64) * 2.0;
    let score = (100.0 - penalty).clamp(0.0, 100.0) as u8;
    let diary = today_diary_id();
    Ok(Health {
        objects: live.len(),
        archived: nodes.len() - live.len(),
        orphans,
        stale,
        undigested_files,
        open_tasks,
        overdue_tasks: overdue,
        unread_bookmarks,
        grown_7d,
        touched_by_agents_7d,
        score,
        today_diary: if nodes.iter().any(|n| n.id == diary) { Some(diary) } else { None },
    })
}

fn is_digested(nodes: &[GraphNode], a: &Attachment) -> bool {
    nodes.iter().find(|n| n.id == a.node_id).map(|n| n.body.contains(&format!("<!-- digest:{} -->", a.id))).unwrap_or(true)
}

/// What needs care most, in order.
pub async fn needs_care(app: &AppState, limit: usize) -> Result<Vec<Care>> {
    let nodes = db::list_nodes(&app.db).await?;
    let edges = db::list_edges(&app.db).await?;
    let atts = db::list_attachments(&app.db).await?;
    let mentioned: std::collections::HashSet<String> = db::all_mentioned_objects(&app.db).await?.into_iter().collect();
    let linked: std::collections::HashSet<&str> = edges.iter().flat_map(|e| [e.source_id.as_str(), e.target_id.as_str()]).collect();
    let mut out = vec![];
    for a in atts.iter().filter(|a| a.text_len > 0 && !is_digested(&nodes, a)) {
        out.push(Care { kind: "digest".into(), node_id: a.node_id.clone(), reason: format!("файл «{}» ещё никто не читал", a.name) });
    }
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    for n in nodes.iter().filter(|n| n.archived == 0 && n.type_id == "task") {
        if n.props.0.get("done").and_then(|d| d.as_bool()) != Some(true) && n.props.0.get("due").and_then(|d| d.as_str()).map(|d| d < today.as_str()).unwrap_or(false) {
            out.push(Care { kind: "overdue".into(), node_id: n.id.clone(), reason: format!("задача «{}» просрочена", n.label) });
        }
    }
    for n in nodes.iter().filter(|n| n.archived == 0 && !linked.contains(n.id.as_str()) && !mentioned.contains(&n.id) && !matches!(n.type_id.as_str(), "feed" | "diary") && n.body.chars().count() > 80) {
        out.push(Care { kind: "orphan".into(), node_id: n.id.clone(), reason: format!("«{}» ни с чем не связан", n.label) });
    }
    for n in nodes.iter().filter(|n| n.archived == 0 && days_since(&n.updated_at) > 30 && matches!(n.type_id.as_str(), "project" | "goal" | "idea")) {
        out.push(Care { kind: "stale".into(), node_id: n.id.clone(), reason: format!("«{}» не трогали {} дн.", n.label, days_since(&n.updated_at)) });
    }
    // de-dup by node, keep first (highest priority)
    let mut seen = std::collections::HashSet::new();
    out.retain(|c| seen.insert(c.node_id.clone()));
    out.truncate(limit);
    Ok(out)
}

/// Which inhabitant should care for an object (role affinity, falls back to anyone alive).
pub fn pick_carer<'a>(agents: &'a [Agent], node: &GraphNode, kind: &str) -> Option<&'a Agent> {
    let alive: Vec<&Agent> = agents.iter().filter(|a| a.is_alive()).collect();
    if alive.is_empty() {
        return None;
    }
    let prefer: &[&str] = match (kind, node.type_id.as_str()) {
        ("digest", _) => &["chronicler", "critic", "engineer", "physicist", "lit_teacher", "mentor"],
        ("overdue", _) => &["producer", "coach", "mentor", "physicist", "hustler"],
        (_, "book") | (_, "quote") => &["lit_teacher", "critic", "chronicler"],
        (_, "project") => &["engineer", "producer", "mentor", "physicist"],
        (_, "idea") => &["dreamer", "graffiti", "critic"],
        (_, "human") => &["shrink", "granny", "lit_teacher"],
        (_, "goal") => &["coach", "producer", "mentor"],
        _ => &["chronicler", "shrink", "lit_teacher", "physicist"],
    };
    for p in prefer {
        if let Some(a) = alive.iter().find(|a| a.role == *p || a.id == *p) {
            return Some(a);
        }
    }
    use rand::seq::SliceRandom;
    alive.choose(&mut rand::thread_rng()).copied()
}

fn opts(max_tokens: u32) -> ChatOpts {
    ChatOpts { temperature: 0.5, max_tokens, json_mode: true, prefill: None, presence_penalty: None, frequency_penalty: None }
}

/// One act of care on `node_id` by `agent`: read it (and its files), improve it, say one line.
pub async fn tend(app: &AppState, agent_id: &str, node_id: &str, kind: &str) -> Result<(Vec<ActionRecord>, Option<String>)> {
    let agents = db::list_agents(&app.db).await?;
    let agent = agents.iter().find(|a| a.id == agent_id).cloned().context("нет такого жителя")?;
    let node = db::get_node(&app.db, node_id).await?.context("нет такого объекта")?;
    let types = db::list_types(&app.db).await?;
    let atts = db::attachments_of(&app.db, node_id).await?;
    let edges = db::edges_of(&app.db, node_id).await?;
    let notes = db::notes_for_node(&app.db, node_id).await?;
    let all = db::list_nodes(&app.db).await?;
    let candidates: Vec<&GraphNode> = {
        let text = format!("{} {}", node.label, mermaid::truncate_chars(&node.body, 400));
        let hits = db::search_objects(&app.db, &text, None, 10).await?;
        let ids: Vec<String> = hits.iter().map(|h| h.id.clone()).filter(|id| id != node_id).collect();
        all.iter().filter(|n| ids.contains(&n.id)).collect()
    };
    let schema = types.iter().find(|t| t.id == node.type_id).and_then(|t| t.schema.0.get("props").cloned()).unwrap_or(json!([]));
    let files_text: String = atts
        .iter()
        .filter(|a| a.text_len > 0)
        .map(|a| format!("### Файл «{}» ({} симв.) [id {}]\n{}", a.name, a.text_len, a.id, mermaid::truncate_chars(&a.text, if kind == "digest" { 7000 } else { 2500 })))
        .collect::<Vec<_>>()
        .join("\n\n");
    let undigested: Vec<&Attachment> = atts.iter().filter(|a| a.text_len > 0 && !node.body.contains(&format!("<!-- digest:{} -->", a.id))).collect();

    let system = format!(
        "{GARDENER_MARK} {core}\n\nСейчас ты не разговариваешь, а ВОЗДЕЛЫВАЕШЬ библиотеку двора: приводишь в порядок один объект. Ты читал его целиком (и приложенные файлы). Верни ТОЛЬКО JSON:\n\
{{\"summary\": \"1–2 предложения, о чём объект (для поля summary; null если не нужно)\",\n \"props\": {{\"ключ\": \"значение по схеме типа — заполняй только пустое и очевидное, null не пиши\"}},\n \"append\": {{\"heading\": \"Конспект|Главное|Лог|Мысли <твоё имя>\", \"text\": \"markdown: что стоит дописать в тело — конспект файла, главные тезисы, следующий шаг, твоё замечание. 3–12 строк, списками\"}},\n \"links\": [{{\"to\": \"id из КАНДИДАТОВ\", \"kind\": \"related|about|belongs_to|blocks|contradicts\", \"why\": \"3 слова\"}}],\n \"new_objects\": [{{\"type\": \"task|human|idea|quote|event|place|book\", \"title\": \"…\", \"props\": {{}}}}],\n \"edits\": [{{\"find\": \"точная фраза из тела (1–2 предложения), которую стоит поправить\", \"replace\": \"как должно быть\", \"why\": \"3–6 слов\"}}],\n \"new_types\": [{{\"name\": \"Название нового типа, если объектам такого рода в библиотеке нет подходящего\", \"icon\": \"эмодзи\", \"props\": [{{\"key\": \"snake_key\", \"label\": \"Подпись\", \"kind\": \"text|number|date|select|rating|checkbox|url|tags\"}}], \"why\": \"зачем\"}}],\n \"note\": \"короткая пометка от тебя в характере или null\",\n \"say\": \"одна реплика в чат от первого лица в твоём характере: что ты сделал и что заметил (1–2 предложения, без списков, с конкретикой из текста)\"}}\n\
Правила: не выдумывай фактов, которых нет в тексте; не переписывай тело сам — правки предлагай через edits (не больше 2, find должен встречаться в теле дословно), а дописывай через append; new_objects — не больше 3 и только если в тексте есть конкретные люди/задачи/даты/цитаты (это ПРЕДЛОЖЕНИЯ, игрок их подтвердит); new_types — максимум один и только если действительно нет подходящего типа; links — только из кандидатов. Пиши по-русски.",
        core = agent.system_core
    );
    let user = format!(
        "ОБЪЕКТ: «{}» [{}] id={}\nСВОЙСТВА: {}\nСХЕМА ТИПА: {}\nТЕЛО:\n{}\n\n{}\n\nСВЯЗИ: {}\nПОМЕТКИ: {}\nКАНДИДАТЫ ДЛЯ СВЯЗЕЙ:\n{}\n\nПОЧЕМУ ТЫ ВЗЯЛСЯ: {}",
        node.label,
        node.type_id,
        node.id,
        node.props.0,
        schema,
        mermaid::truncate_chars(&node.body, 3500),
        if files_text.is_empty() { "(файлов нет)".to_string() } else { files_text },
        if edges.is_empty() { "нет".to_string() } else { edges.iter().map(|e| format!("{}→{} ({})", e.source_id, e.target_id, e.kind)).collect::<Vec<_>>().join(", ") },
        if notes.is_empty() { "нет".to_string() } else { notes.iter().map(|n| format!("{}: {}", n.author, mermaid::truncate_chars(&n.text, 100))).collect::<Vec<_>>().join(" | ") },
        if candidates.is_empty() { "  (нет)".to_string() } else { candidates.iter().map(|c| format!("  {} [{}] «{}»", c.id, c.type_id, c.label)).collect::<Vec<_>>().join("\n") },
        match kind { "digest" => "к объекту приложили файл, его никто ещё не читал", "overdue" => "задача просрочена", "orphan" => "объект ни с чем не связан", "stale" => "объект давно не трогали", _ => "игрок попросил разобрать" }
    );
    let settings = app.llm_settings().await;
    let model = if agent.model.trim().is_empty() { settings.director_model.clone() } else { agent.model.clone() };
    app.emit(RoomEvent::Status { stage: "agent".into(), detail: format!("{} разбирает «{}»", agent.name, mermaid::truncate_chars(&node.label, 30)) });
    let raw = app.llm.chat(&model, &[ChatMessage::system(system), ChatMessage::user(user)], &opts(1100)).await?;
    let plan = extract_json(&raw)?;

    let mut records: Vec<ActionRecord> = vec![];
    // props + summary
    let mut props = plan.get("props").and_then(|p| p.as_object()).cloned().unwrap_or_default();
    if let Some(s) = plan.get("summary").and_then(|s| s.as_str()).filter(|s| !s.trim().is_empty()) {
        let has_summary = schema.as_array().map(|a| a.iter().any(|p| p.get("key").and_then(|k| k.as_str()) == Some("summary"))).unwrap_or(false);
        if has_summary && node.props.0.get("summary").and_then(|x| x.as_str()).map(|x| x.trim().is_empty()).unwrap_or(true) {
            props.insert("summary".into(), json!(mermaid::truncate_chars(s.trim(), 400)));
        }
    }
    props.retain(|k, v| !v.is_null() && node.props.0.get(k).map(|cur| cur.is_null() || cur.as_str() == Some("")).unwrap_or(true));
    if !props.is_empty() {
        let (_, rec) = crate::tools::call(app, &agent.id, "library_update", &json!({ "id": node.id, "props": Value::Object(props) })).await?;
        records.extend(rec);
    }
    // append (with digest markers for files we read)
    if let Some(ap) = plan.get("append").filter(|a| a.is_object()) {
        let text = ap.get("text").and_then(|t| t.as_str()).unwrap_or("").trim();
        if text.chars().count() > 10 {
            let heading = ap.get("heading").and_then(|h| h.as_str()).unwrap_or("Главное");
            let markers: String = undigested.iter().map(|a| format!("\n<!-- digest:{} -->", a.id)).collect();
            let signed = format!("{text}\n\n_— {}_{markers}", agent.name.split(" (").next().unwrap_or(&agent.name));
            let (_, rec) = crate::tools::call(app, &agent.id, "library_append", &json!({ "id": node.id, "heading": heading, "text": signed })).await?;
            records.extend(rec);
        }
    } else if !undigested.is_empty() {
        // nothing to append but the files were read: mark them so we don't loop
        let markers: String = undigested.iter().map(|a| format!("<!-- digest:{} -->", a.id)).collect::<Vec<_>>().join("\n");
        db::update_node_fields(&app.db, &node.id, None, Some(&format!("{}\n\n{markers}", node.body.trim_end())), None, None).await?;
    }
    // links
    for l in plan.get("links").and_then(|l| l.as_array()).map(|a| a.iter().take(4).collect::<Vec<_>>()).unwrap_or_default() {
        let to = l.get("to").and_then(|t| t.as_str()).unwrap_or("");
        if candidates.iter().any(|c| c.id == to) {
            let (_, rec) = crate::tools::call(app, &agent.id, "library_link", &json!({ "from": node.id, "to": to, "kind": l.get("kind").cloned().unwrap_or(json!("related")), "label": l.get("why").cloned().unwrap_or(json!("")) })).await?;
            records.extend(rec);
        }
    }
    // new objects / types are PROPOSED, not created: you accept them in the chat
    let mut proposal_ids: Vec<i64> = vec![];
    for o in plan.get("new_objects").and_then(|l| l.as_array()).map(|a| a.iter().take(3).collect::<Vec<_>>()).unwrap_or_default() {
        let title = o.get("title").and_then(|t| t.as_str()).unwrap_or("").trim();
        if title.is_empty() {
            continue;
        }
        let payload = json!({ "type": o.get("type").cloned().unwrap_or(json!("note")), "title": title, "props": o.get("props").cloned().unwrap_or(json!({})), "link_to": node.id });
        proposal_ids.push(db::insert_proposal(&app.db, &agent.id, "create", &payload, &format!("из разбора «{}»", node.label), None).await?);
    }
    for e in plan.get("edits").and_then(|l| l.as_array()).map(|a| a.iter().take(2).collect::<Vec<_>>()).unwrap_or_default() {
        let find = e.get("find").and_then(|x| x.as_str()).unwrap_or("").trim();
        let replace = e.get("replace").and_then(|x| x.as_str()).unwrap_or("").trim();
        if find.chars().count() >= 8 && !replace.is_empty() && find != replace && node.body.contains(find) {
            proposal_ids.push(db::insert_proposal(&app.db, &agent.id, "edit", &json!({ "id": node.id, "find": find, "replace": replace }), e.get("why").and_then(|x| x.as_str()).unwrap_or("правка"), None).await?);
        }
    }
    for t in plan.get("new_types").and_then(|l| l.as_array()).map(|a| a.iter().take(1).collect::<Vec<_>>()).unwrap_or_default() {
        let name = t.get("name").and_then(|x| x.as_str()).unwrap_or("").trim();
        if name.chars().count() >= 3 {
            proposal_ids.push(db::insert_proposal(&app.db, &agent.id, "new_type", &json!({ "name": name, "icon": t.get("icon").cloned().unwrap_or(json!("◇")), "props": t.get("props").cloned().unwrap_or(json!([])), "why": t.get("why").cloned().unwrap_or(json!("")) }), &format!("пока разбирал «{}»", node.label), None).await?);
        }
    }
    if let Some(n) = plan.get("note").and_then(|n| n.as_str()).filter(|n| n.trim().chars().count() > 3) {
        let (_, rec) = crate::tools::call(app, &agent.id, "library_note", &json!({ "id": node.id, "text": n.trim() })).await?;
        records.extend(rec);
    }
    // the line in chat
    let say = plan.get("say").and_then(|s| s.as_str()).map(str::trim).filter(|s| !s.is_empty()).map(|s| mermaid::truncate_chars(s, 600));
    if say.is_some() || !proposal_ids.is_empty() {
        let text = say.clone().unwrap_or_else(|| format!("Разобрал «{}». Есть пара предложений — смотри ниже.", node.label));
        let msg = db::insert_message_full(&app.db, &agent.id, None, &text, Some("user"), Some("забота"), Some(&node.id)).await?;
        db::set_message_actions(&app.db, msg.id, &records).await?;
        db::add_mention(&app.db, msg.id, &node.id).await?;
        for pid in &proposal_ids {
            db::set_proposal_message(&app.db, *pid, msg.id).await?;
        }
        let full = db::with_mentions(&app.db, vec![db::get_message(&app.db, msg.id).await?.unwrap()]).await?.remove(0);
        app.emit(RoomEvent::Message { message: full });
        if !proposal_ids.is_empty() {
            app.emit(RoomEvent::Proposals { proposals: db::list_proposals(&app.db, Some("pending"), 30).await? });
        }
    }
    db::insert_event(&app.db, "tended", &json!({ "agent": agent.id, "node": node.id, "kind": kind, "actions": records.len() })).await?;
    crate::handlers::emit_graph_pub(app).await?;
    app.emit(RoomEvent::Notes { notes: db::list_notes(&app.db, 400).await? });
    Ok((records, say))
}

/// Pick the most needed object and tend it. Returns what was done, if anything.
pub async fn tend_next(app: &AppState) -> Result<Option<(String, String, usize)>> {
    let Some(care) = needs_care(app, 1).await?.into_iter().next() else { return Ok(None) };
    let agents = db::list_agents(&app.db).await?;
    let node = db::get_node(&app.db, &care.node_id).await?.context("node vanished")?;
    let Some(agent) = pick_carer(&agents, &node, &care.kind) else { return Ok(None) };
    let (records, _) = tend(app, &agent.id, &care.node_id, &care.kind).await?;
    Ok(Some((agent.id.clone(), care.node_id, records.len())))
}

/// The day's chronicle: a diary object the yard writes about itself.
pub async fn chronicle(app: &AppState, force: bool) -> Result<Option<String>> {
    let id = today_diary_id();
    let date = chrono::Local::now().format("%d.%m.%Y").to_string();
    if !force && db::node_exists(&app.db, &id).await? {
        return Ok(None);
    }
    let since = (chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339();
    let nodes = db::list_nodes(&app.db).await?;
    let agents = db::list_agents(&app.db).await?;
    let messages = db::recent_messages(&app.db, 120).await?;
    let day_msgs: Vec<_> = messages.iter().filter(|m| m.created_at > since).collect();
    if day_msgs.len() < 2 && !force {
        return Ok(None);
    }
    let new_objs: Vec<_> = nodes.iter().filter(|n| n.created_at > since && n.id != id).collect();
    let done: Vec<_> = nodes.iter().filter(|n| n.type_id == "task" && n.updated_at > since && n.props.0.get("done").and_then(|d| d.as_bool()) == Some(true)).collect();
    let contracts = db::list_contracts(&app.db).await?;
    let events = db::recent_events(&app.db, 30).await?;
    let facts = format!(
        "- реплик за сутки: {} (игрок: {})\n- новые объекты: {}\n- закрытые задачи: {}\n- открытые слова: {}\n- события: {}",
        day_msgs.len(),
        day_msgs.iter().filter(|m| m.sender_id == "user").count(),
        if new_objs.is_empty() { "нет".into() } else { new_objs.iter().take(12).map(|n| format!("[[{}]]", n.label)).collect::<Vec<_>>().join(", ") },
        if done.is_empty() { "нет".into() } else { done.iter().map(|n| format!("[[{}]]", n.label)).collect::<Vec<_>>().join(", ") },
        contracts.iter().filter(|c| c.status == "open").map(|c| format!("«{}»", c.title)).collect::<Vec<_>>().join(", "),
        events.iter().filter(|e| e.created_at > since).map(|e| e.kind.clone()).collect::<Vec<_>>().join(", ")
    );
    let writer = agents.iter().filter(|a| a.is_alive()).find(|a| a.role == "chronicler").or_else(|| agents.iter().find(|a| a.is_alive())).cloned();
    let transcript = crate::prompts::transcript(&day_msgs.iter().rev().take(40).rev().cloned().cloned().collect::<Vec<_>>(), &agents, false);
    let voice = writer.as_ref().map(|w| format!("Пишет {} — в его/её голосе, но сдержанно, как летопись, не как реплика.", w.name)).unwrap_or_else(|| "Пишет сам двор, нейтрально.".into());
    let system = format!("{CHRONICLE_MARK} Ты ведёшь хронику двора — живой комнаты, где игрок разговаривает с несколькими персонажами и ведёт общую библиотеку. {voice} Верни ТОЛЬКО JSON: {{\"title\": \"заголовок дня (3–6 слов)\", \"text\": \"markdown 6–14 строк: что было главным, что игрок решил или обещал, что сдвинулось в библиотеке, какая тема повисла; с [[вики-ссылками]] на объекты из фактов; без воды и без оценок игрока как человека\", \"mood\": 1–5, \"tomorrow\": \"одна строка — на что завтра обратить внимание\"}}");
    let user = format!("ДАТА: {date}\nФАКТЫ:\n{facts}\n\nСТЕНОГРАММА ДНЯ (последние реплики):\n{transcript}");
    let model = app.llm_settings().await.director_model;
    let raw = app.llm.chat(&model, &[ChatMessage::system(system), ChatMessage::user(user)], &opts(900)).await?;
    let v = extract_json(&raw)?;
    let title = format!("Хроника двора · {date}{}", v.get("title").and_then(|t| t.as_str()).map(|t| format!(" · {t}")).unwrap_or_default());
    let text = v.get("text").and_then(|t| t.as_str()).unwrap_or("").trim().to_string();
    let tomorrow = v.get("tomorrow").and_then(|t| t.as_str()).map(|t| format!("\n\n## Завтра\n\n{t}")).unwrap_or_default();
    let body = format!("{text}\n\n## Факты\n\n{facts}{tomorrow}\n\n_— {}_", writer.as_ref().map(|w| w.name.clone()).unwrap_or_else(|| "двор".into()));
    let author = writer.as_ref().map(|w| w.id.clone()).unwrap_or_else(|| "system".into());
    let props = json!({ "date": chrono::Local::now().format("%Y-%m-%d").to_string(), "mood": v.get("mood").and_then(|m| m.as_i64()).unwrap_or(3), "tags": ["хроника"] });
    db::upsert_object(&app.db, &id, &mermaid::sanitize_label(&title), "diary", &author, Some(&mermaid::truncate_chars(&body, 20_000)), Some(&props), None, None).await?;
    for n in new_objs.iter().take(12).chain(done.iter().take(12)) {
        db::insert_relation(&app.db, &id, &n.id, "about", "", &author, 1.0).await?;
    }
    db::place_unpositioned_nodes(&app.db).await?;
    if let Some(w) = &writer {
        let line = format!("Записал хронику дня: «{}». {}", title, v.get("tomorrow").and_then(|t| t.as_str()).unwrap_or(""));
        let msg = db::insert_message_full(&app.db, &w.id, None, &line, Some("room"), Some("хроника"), Some(&id)).await?;
        db::add_mention(&app.db, msg.id, &id).await?;
        db::set_message_actions(&app.db, msg.id, &[ActionRecord { op: "create".into(), object_id: id.clone(), text: "записал хронику дня".into() }]).await?;
        let full = db::with_mentions(&app.db, vec![db::get_message(&app.db, msg.id).await?.unwrap()]).await?.remove(0);
        app.emit(RoomEvent::Message { message: full });
    }
    db::insert_event(&app.db, "chronicle", &json!({ "id": id })).await?;
    crate::handlers::emit_graph_pub(app).await?;
    Ok(Some(id))
}

/// An inhabitant moves their own project forward in writing (after an own_project beat).
pub async fn advance_project(app: &AppState, agent_id: &str, said: &str) -> Result<()> {
    let projects = db::objects_by_owner(&app.db, agent_id).await?;
    let Some(p) = projects.into_iter().find(|n| n.type_id == "project" && n.archived == 0) else { return Ok(()) };
    let agents = db::list_agents(&app.db).await?;
    let name = agents.iter().find(|a| a.id == agent_id).map(|a| a.name.clone()).unwrap_or_default();
    let line = format!("{}: {}", chrono::Local::now().format("%d.%m"), mermaid::truncate_chars(said.trim(), 300));
    let _ = crate::tools::call(app, agent_id, "library_append", &json!({ "id": p.id, "heading": "Лог", "text": line })).await?;
    tracing::debug!(agent = %name, project = %p.id, "project log advanced");
    Ok(())
}

/// Recent things the inhabitants did in the library (from message action records).
pub async fn recent_care(app: &AppState, limit: usize) -> Result<Vec<Value>> {
    let msgs = db::recent_messages(&app.db, 200).await?;
    let mut out = vec![];
    for m in msgs.iter().rev() {
        if m.sender_id == "user" || m.sender_id == "system" {
            continue;
        }
        for a in m.actions.0.iter() {
            out.push(json!({ "at": m.created_at, "agent": m.sender_id, "op": a.op, "object_id": a.object_id, "text": a.text, "message_id": m.id }));
            if out.len() >= limit {
                return Ok(out);
            }
        }
    }
    Ok(out)
}

// ───────────────────────────── Proposals: apply & audit ─────────────────────────────

/// Do what a proposal says (after the player accepted it). Returns a short human result.
pub async fn apply_proposal(app: &AppState, p: &crate::models::Proposal) -> Result<String> {
    let v = &p.payload.0;
    let actor = p.agent_id.as_str();
    match p.kind.as_str() {
        "merge" => {
            let src = v.get("src").and_then(|x| x.as_str()).context("src")?;
            let dst = v.get("dst").and_then(|x| x.as_str()).context("dst")?;
            db::merge_objects(&app.db, src, dst).await?;
            Ok(format!("«{src}» влит в «{dst}», исходный в архиве"))
        }
        "archive" => {
            let id = v.get("id").and_then(|x| x.as_str()).context("id")?;
            db::update_object_meta(&app.db, id, None, None, Some(true)).await?;
            Ok(format!("«{id}» в архиве"))
        }
        "tag" => {
            let id = v.get("id").and_then(|x| x.as_str()).context("id")?;
            let tags = v.get("tags").cloned().unwrap_or(json!([]));
            let node = db::get_node(&app.db, id).await?.context("объект")?;
            let mut cur: Vec<Value> = node.props.0.get("tags").and_then(|t| t.as_array()).cloned().unwrap_or_default();
            for t in tags.as_array().cloned().unwrap_or_default() {
                if !cur.contains(&t) {
                    cur.push(t);
                }
            }
            db::update_object_meta(&app.db, id, None, Some(&json!({ "tags": cur })), None).await?;
            Ok(format!("теги обновлены у «{}»", node.label))
        }
        "link" => {
            let (_, rec) = crate::tools::call(app, actor, "library_link", v).await?;
            Ok(rec.map(|r| r.text).unwrap_or_else(|| "связано".into()))
        }
        "create" => {
            let (res, rec) = crate::tools::call(app, actor, "library_create", &json!({ "type": v.get("type").cloned().unwrap_or(json!("note")), "title": v.get("title").cloned().unwrap_or(json!("")), "props": v.get("props").cloned().unwrap_or(json!({})), "body": v.get("body").cloned().unwrap_or(json!("")) })).await?;
            if let (Some(id), Some(to)) = (res.get("id").and_then(|i| i.as_str()), v.get("link_to").and_then(|x| x.as_str())) {
                let _ = crate::tools::call(app, actor, "library_link", &json!({ "from": id, "to": to, "kind": "about", "label": "из разбора" })).await;
            }
            Ok(rec.map(|r| r.text).unwrap_or_else(|| "создано".into()))
        }
        "new_type" => {
            let name = v.get("name").and_then(|x| x.as_str()).context("name")?;
            let id = mermaid::safe_id(name);
            if db::type_exists(&app.db, &id).await? {
                anyhow::bail!("такой тип уже есть");
            }
            let props = v.get("props").cloned().unwrap_or(json!([]));
            db::insert_type(&app.db, &id, name, v.get("icon").and_then(|x| x.as_str()).unwrap_or("◇"), "#9a9ca8", &json!({ "props": props }), actor).await?;
            Ok(format!("тип «{name}» заведён"))
        }
        "edit" => {
            let id = v.get("id").and_then(|x| x.as_str()).context("id")?;
            let find = v.get("find").and_then(|x| x.as_str()).context("find")?;
            let replace = v.get("replace").and_then(|x| x.as_str()).context("replace")?;
            let node = db::get_node(&app.db, id).await?.context("объект")?;
            anyhow::ensure!(node.body.contains(find), "этот фрагмент уже изменён — правка устарела");
            let body = node.body.replacen(find, replace, 1);
            db::update_node_fields(&app.db, id, None, Some(&body), None, None).await?;
            Ok(format!("правка в «{}» применена", node.label))
        }
        "retitle" => {
            let id = v.get("id").and_then(|x| x.as_str()).context("id")?;
            let title = v.get("title").and_then(|x| x.as_str()).context("title")?;
            db::update_node_fields(&app.db, id, Some(&mermaid::sanitize_label(title)), None, None, None).await?;
            Ok(format!("переименовано в «{title}»"))
        }
        "retype" => {
            let id = v.get("id").and_then(|x| x.as_str()).context("id")?;
            let ty = v.get("type").and_then(|x| x.as_str()).context("type")?;
            db::update_object_meta(&app.db, id, Some(ty), None, None).await?;
            Ok(format!("тип сменён на «{ty}»"))
        }
        other => anyhow::bail!("неизвестный вид предложения {other}"),
    }
}

fn norm_label(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

fn jaccard(a: &str, b: &str) -> f64 {
    let sa: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let sb: std::collections::HashSet<&str> = b.split_whitespace().collect();
    if sa.is_empty() || sb.is_empty() {
        return 0.0;
    }
    sa.intersection(&sb).count() as f64 / sa.union(&sb).count() as f64
}

/// Audit: duplicates, junk, untyped/untagged — one JSON call confirms, then the carer asks you
/// in the chat with accept/reject chips. Returns the number of proposals made.
pub async fn audit(app: &AppState) -> Result<usize> {
    let nodes: Vec<GraphNode> = db::list_nodes(&app.db).await?.into_iter().filter(|n| n.archived == 0).collect();
    let agents = db::list_agents(&app.db).await?;
    let types = db::list_types(&app.db).await?;
    let pending = db::list_proposals(&app.db, Some("pending"), 100).await?;
    let already: std::collections::HashSet<String> = pending.iter().map(|p| p.payload.0.to_string()).collect();
    // candidates
    let mut dupes: Vec<(String, String, f64)> = vec![];
    for i in 0..nodes.len() {
        for j in (i + 1)..nodes.len() {
            let (a, b) = (&nodes[i], &nodes[j]);
            let (na, nb) = (norm_label(&a.label), norm_label(&b.label));
            let sim = if na == nb { 1.0 } else { jaccard(&na, &nb) };
            if sim >= 0.6 && a.type_id == b.type_id || sim >= 0.85 {
                dupes.push((a.id.clone(), b.id.clone(), sim));
            }
        }
    }
    dupes.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    dupes.truncate(6);
    let edges = db::list_edges(&app.db).await?;
    let mentioned: std::collections::HashSet<String> = db::all_mentioned_objects(&app.db).await?.into_iter().collect();
    let junk: Vec<&GraphNode> = nodes
        .iter()
        .filter(|n| n.body.trim().is_empty() && n.props.0.as_object().map(|o| o.values().all(|v| v.is_null() || v.as_str() == Some(""))).unwrap_or(true) && !edges.iter().any(|e| e.source_id == n.id || e.target_id == n.id) && !mentioned.contains(&n.id) && days_since(&n.created_at) >= 3 && !matches!(n.type_id.as_str(), "project" | "feed" | "diary"))
        .take(6)
        .collect();
    let untagged: Vec<&GraphNode> = nodes.iter().filter(|n| n.props.0.get("tags").and_then(|t| t.as_array()).map(|a| a.is_empty()).unwrap_or(true) && n.body.chars().count() > 120).take(8).collect();
    if dupes.is_empty() && junk.is_empty() && untagged.is_empty() {
        return Ok(0);
    }
    let carer = agents.iter().filter(|a| a.is_alive()).find(|a| a.role == "chronicler").or_else(|| agents.iter().find(|a| a.is_alive())).cloned().context("некому")?;
    let desc = |id: &str| nodes.iter().find(|n| n.id == id).map(|n| format!("{} [{}] «{}»{}", n.id, n.type_id, n.label, if n.body.is_empty() { String::new() } else { format!(": {}", mermaid::truncate_chars(&n.body, 100)) })).unwrap_or_else(|| id.to_string());
    let system = format!("{CHRONICLE_MARK}[AUDIT] {}\n\nТы наводишь порядок в библиотеке двора и предлагаешь игроку изменения — он подтвердит или отклонит каждое. Верни ТОЛЬКО JSON: {{\"proposals\": [{{\"kind\": \"merge|archive|tag|retype\", \"src\": \"id (для merge: что влить)\", \"dst\": \"id (куда)\", \"id\": \"id (для archive/tag/retype)\", \"tags\": [\"тег\"], \"type\": \"id типа (retype)\", \"reason\": \"одна фраза, почему\"}}], \"say\": \"одна реплика игроку в характере: что нашёл и что предлагаешь, 1–3 предложения, без списков\"}}. Правила: предлагай только из КАНДИДАТОВ; merge — только если это действительно одно и то же; archive — только пустышки без связей; теги — 1–3 коротких слова в нижнем регистре; максимум 6 предложений; если сомневаешься — не предлагай.", carer.system_core);
    let user = format!(
        "ТИПЫ: {}\nВОЗМОЖНЫЕ ДУБЛИ:\n{}\nПУСТЫШКИ (без тела, свойств, связей, старше 3 дней):\n{}\nБЕЗ ТЕГОВ:\n{}",
        types.iter().map(|t| t.id.clone()).collect::<Vec<_>>().join(", "),
        if dupes.is_empty() { "  нет".into() } else { dupes.iter().map(|(a, b, s)| format!("  {:.0}%: {} ⇄ {}", s * 100.0, desc(a), desc(b))).collect::<Vec<_>>().join("\n") },
        if junk.is_empty() { "  нет".into() } else { junk.iter().map(|n| format!("  {}", desc(&n.id))).collect::<Vec<_>>().join("\n") },
        if untagged.is_empty() { "  нет".into() } else { untagged.iter().map(|n| format!("  {}", desc(&n.id))).collect::<Vec<_>>().join("\n") }
    );
    let model = app.llm_settings().await.director_model;
    app.emit(RoomEvent::Status { stage: "agent".into(), detail: format!("{} наводит порядок в библиотеке…", carer.name) });
    let raw = app.llm.chat(&model, &[ChatMessage::system(system), ChatMessage::user(user)], &opts(900)).await?;
    let v = extract_json(&raw)?;
    let mut ids: Vec<i64> = vec![];
    let known = |id: &str| nodes.iter().any(|n| n.id == id);
    for p in v.get("proposals").and_then(|p| p.as_array()).map(|a| a.iter().take(6).collect::<Vec<_>>()).unwrap_or_default() {
        let kind = p.get("kind").and_then(|k| k.as_str()).unwrap_or("");
        let reason = p.get("reason").and_then(|r| r.as_str()).unwrap_or("").to_string();
        let payload = match kind {
            "merge" => {
                let (src, dst) = (p.get("src").and_then(|x| x.as_str()).unwrap_or(""), p.get("dst").and_then(|x| x.as_str()).unwrap_or(""));
                if src == dst || !known(src) || !known(dst) || !dupes.iter().any(|(a, b, _)| (a == src && b == dst) || (a == dst && b == src)) {
                    continue;
                }
                json!({ "src": src, "dst": dst })
            }
            "archive" => {
                let id = p.get("id").and_then(|x| x.as_str()).unwrap_or("");
                if !junk.iter().any(|n| n.id == id) {
                    continue;
                }
                json!({ "id": id })
            }
            "tag" => {
                let id = p.get("id").and_then(|x| x.as_str()).unwrap_or("");
                let tags: Vec<String> = p.get("tags").and_then(|t| t.as_array()).map(|a| a.iter().filter_map(|x| x.as_str()).map(|x| x.trim().to_lowercase()).filter(|x| !x.is_empty()).take(3).collect()).unwrap_or_default();
                if !known(id) || tags.is_empty() {
                    continue;
                }
                json!({ "id": id, "tags": tags })
            }
            "retype" => {
                let id = p.get("id").and_then(|x| x.as_str()).unwrap_or("");
                let ty = p.get("type").and_then(|x| x.as_str()).unwrap_or("");
                if !known(id) || !types.iter().any(|t| t.id == ty) {
                    continue;
                }
                json!({ "id": id, "type": ty })
            }
            _ => continue,
        };
        if already.contains(&payload.to_string()) {
            continue;
        }
        ids.push(db::insert_proposal(&app.db, &carer.id, kind, &payload, &reason, None).await?);
    }
    if ids.is_empty() {
        return Ok(0);
    }
    let say = v.get("say").and_then(|s| s.as_str()).map(str::trim).filter(|s| !s.is_empty()).map(|s| mermaid::truncate_chars(s, 500)).unwrap_or_else(|| format!("Прошёлся по библиотеке — есть {} предложений, глянь.", ids.len()));
    let msg = db::insert_message_full(&app.db, &carer.id, None, &say, Some("user"), Some("забота"), None).await?;
    for id in &ids {
        db::set_proposal_message(&app.db, *id, msg.id).await?;
    }
    app.emit(RoomEvent::Message { message: msg });
    app.emit(RoomEvent::Proposals { proposals: db::list_proposals(&app.db, Some("pending"), 30).await? });
    db::insert_event(&app.db, "audit", &json!({ "agent": carer.id, "proposals": ids.len() })).await?;
    Ok(ids.len())
}
