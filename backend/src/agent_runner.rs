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

pub async fn run_agent(app: &AppState, agent_id: &str, room_mood: Option<&str>) -> Result<Message> {
    let agents = db::list_agents(&app.db).await?;
    let agent = agents
        .iter()
        .find(|a| a.id == agent_id)
        .cloned()
        .ok_or_else(|| anyhow!("unknown agent {agent_id}"))?;
    if !agent.is_alive() {
        return Err(anyhow!("{} is dead and cannot speak", agent.name));
    }
    let others: Vec<_> = agents.iter().filter(|a| a.id != agent_id).cloned().collect();
    let messages = db::recent_messages(&app.db, app.cfg.context_messages).await?;
    let nodes = db::list_nodes(&app.db).await?;
    let last_ego = db::last_ego_thought(&app.db, agent_id).await?;

    let ctx = AgentContext {
        agent: &agent,
        others: &others,
        messages: &messages,
        nodes: &nodes,
        last_ego: last_ego.as_deref(),
        room_mood,
    };
    let msgs = [
        ChatMessage::system(prompts::agent_system(&ctx)),
        ChatMessage::user(prompts::agent_user(&ctx, &agents)),
    ];
    let opts = ChatOpts {
        temperature: 0.95,
        max_tokens: 700,
        json_mode: false,
        prefill: Some(EGO_PREFILL.to_string()),
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
    let msg = db::insert_message(&app.db, agent_id, ego.as_deref(), &speech).await?;
    app.emit(RoomEvent::Message { message: msg.clone() });
    Ok(msg)
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
    fn handles_missing_prefill_and_label() {
        let (ego, speech) = parse_agent_reply("мысль</ego_thought>\n**Публичная речь:** Говорю.");
        assert_eq!(ego.as_deref(), Some("мысль"));
        assert_eq!(speech, "Говорю.");
    }
}
