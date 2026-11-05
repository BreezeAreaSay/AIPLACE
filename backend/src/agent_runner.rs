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
    };
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

    let model = app.llm_settings().await.model;
    let (ego, speech) = match app.llm.chat(&model, &msgs, &opts).await {
        Ok(raw) => parse_agent_reply(&raw),
        Err(e) => {
            tracing::warn!(agent = %agent_id, error = %e, "agent LLM call failed");
            (
                Some(format!("(связь с моделью оборвалась: {})", crate::mermaid::truncate_chars(&e.to_string(), 120))),
                silence_line(&agent.id),
            )
        }
    };
    let speech = strip_name_prefix(&speech, &agent.name);
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
        _ => "…(Кэп стучит по рации: «алло, база, палево, сигнал пропал».)".into(),
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
