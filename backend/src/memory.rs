//! Long-term memory: each agent keeps a first-person diary that a fast model rewrites every
//! few messages. It is injected into agent and Director prompts, so nothing is forgotten when
//! the rolling transcript window scrolls past.

use anyhow::Result;

use crate::{
    db,
    models::RoomEvent,
    openrouter::{ChatMessage, ChatOpts},
    prompts,
    state::AppState,
};

/// Spawn consolidation in the background if enough new messages accumulated.
pub async fn maybe_consolidate(app: &AppState) -> Result<()> {
    let latest = db::last_message_id(&app.db).await?;
    let agents = db::list_agents(&app.db).await?;
    let mut due = Vec::new();
    for a in agents.iter().filter(|a| a.is_alive()) {
        let covered = db::get_memory(&app.db, &a.id).await?.map(|m| m.covered_until).unwrap_or(0);
        if latest - covered >= app.cfg.memory_every as i64 {
            due.push(a.id.clone());
        }
    }
    if due.is_empty() {
        return Ok(());
    }
    let app = app.clone();
    tokio::spawn(async move {
        for id in due {
            if let Err(e) = consolidate_agent(&app, &id, latest).await {
                tracing::warn!(agent = %id, error = %e, "memory consolidation failed");
            }
        }
        if let Ok(memories) = db::list_memories(&app.db).await {
            app.emit(RoomEvent::Memories { memories });
        }
    });
    Ok(())
}

pub async fn consolidate_agent(app: &AppState, agent_id: &str, until: i64) -> Result<()> {
    let agents = db::list_agents(&app.db).await?;
    let Some(agent) = agents.iter().find(|a| a.id == agent_id) else { return Ok(()) };
    let prev = db::get_memory(&app.db, agent_id).await?;
    let after = prev.as_ref().map(|m| m.covered_until).unwrap_or(0);
    let new_msgs = db::messages_between(&app.db, after, until, 40).await?;
    if new_msgs.is_empty() {
        return Ok(());
    }
    let contracts = db::open_contracts(&app.db).await?;
    let relations = db::list_relations(&app.db).await?;
    let msgs = [
        ChatMessage::system(prompts::memory_system(agent)),
        ChatMessage::user(prompts::memory_user(
            agent,
            prev.as_ref().map(|m| m.summary.as_str()).unwrap_or(""),
            &new_msgs,
            &agents,
            &contracts,
            &relations,
        )),
    ];
    let opts = ChatOpts { temperature: 0.5, max_tokens: 450, json_mode: false, prefill: None, presence_penalty: None, frequency_penalty: None };
    let model = app.llm_settings().await.director_model;
    let summary = app.llm.chat(&model, &msgs, &opts).await?;
    let summary = crate::mermaid::truncate_chars(summary.trim().trim_matches('"'), 1200);
    if summary.is_empty() {
        return Ok(());
    }
    db::upsert_memory(&app.db, agent_id, &summary, until).await?;
    tracing::info!(agent = %agent_id, until, "diary updated");
    Ok(())
}
