//! Serde / sqlx models shared by the DB layer, the LLM layer and the HTTP API.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const USER_ID: &str = "user";
pub const NODE_TYPES: [&str; 4] = ["concept", "task", "conflict", "insight"];

// ───────────────────────────── Agents ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub archetype: String,
    pub system_core: String,
    pub hp: i64,
    pub dopamine: i64,
    pub current_agenda: String,
    pub respect_user: i64,
    pub color: String,
}

impl Agent {
    pub fn is_alive(&self) -> bool {
        self.hp > 0
    }
}

// ───────────────────────────── Messages ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Message {
    pub id: i64,
    pub sender_id: String,
    pub ego_thought: Option<String>,
    pub content: String,
    pub created_at: String,
}

// ───────────────────────────── Graph ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub node_type: String,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct GraphEdge {
    pub id: i64,
    pub source_id: String,
    pub target_id: String,
    pub relation_label: String,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

// ───────────────────────────── Contracts / relations / memory ─────────────────────────────

/// A promise the player made. One agent guarantees it with their own HP.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Contract {
    pub id: i64,
    pub title: String,
    pub details: Option<String>,
    /// RFC3339 UTC; `None` = no deadline (open-ended word).
    pub deadline: Option<String>,
    /// open | done | failed | cancelled
    pub status: String,
    pub guarantor_id: Option<String>,
    pub stake: i64,
    pub created_at: String,
    pub resolved_at: Option<String>,
    pub resolution_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Relation {
    pub a_id: String,
    pub b_id: String,
    /// -100 (ненависть) .. 100 (союз)
    pub affinity: i64,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AgentMemory {
    pub agent_id: String,
    pub summary: String,
    pub covered_until: i64,
    pub updated_at: String,
}

/// Mechanical fact recorded by code. Never shown as prose by code — the Director narrates it.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RoomEventRecord {
    pub id: i64,
    pub kind: String,
    pub payload: String,
    pub narrated: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResolveContractRequest {
    pub status: String,
    #[serde(default)]
    pub note: Option<String>,
}

// ───────────────────────────── Director output (LLM JSON) ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NewContract {
    pub title: String,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub deadline: Option<String>,
    #[serde(alias = "guarantor_id", default)]
    pub guarantor: String,
    #[serde(default)]
    pub stake: Option<f64>,
    #[serde(default)]
    pub why: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContractUpdate {
    #[serde(default)]
    pub id: serde_json::Value,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RelationDelta {
    pub a: String,
    pub b: String,
    #[serde(default)]
    pub delta: f64,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatDelta {
    #[serde(default)]
    pub hp: f64,
    #[serde(default)]
    pub dopamine: f64,
    #[serde(default)]
    pub respect_user: f64,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NewNode {
    #[serde(default)]
    pub id: String,
    pub label: String,
    #[serde(rename = "type", alias = "node_type", default)]
    pub node_type: String,
    #[serde(alias = "created_by", default)]
    pub author: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NewEdge {
    #[serde(alias = "source", alias = "source_id")]
    pub from: String,
    #[serde(alias = "target", alias = "target_id")]
    pub to: String,
    #[serde(alias = "relation_label", alias = "relation", default)]
    pub label: String,
    #[serde(alias = "created_by", default)]
    pub author: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphMutations {
    #[serde(default)]
    pub nodes: Vec<NewNode>,
    #[serde(default)]
    pub edges: Vec<NewEdge>,
}

/// Strict shape the Meta-Director must return. Every field is defaulted so a
/// partially-valid JSON still yields a usable plan.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DirectorOutput {
    #[serde(default)]
    pub room_mood: Option<String>,
    #[serde(default)]
    pub director_note: Option<String>,
    #[serde(default)]
    pub next_speakers: Vec<String>,
    #[serde(default)]
    pub updated_agendas: HashMap<String, String>,
    #[serde(default)]
    pub stat_deltas: HashMap<String, StatDelta>,
    #[serde(default)]
    pub graph_mutations: GraphMutations,
    #[serde(default)]
    pub new_contracts: Vec<NewContract>,
    #[serde(default)]
    pub contract_updates: Vec<ContractUpdate>,
    #[serde(default)]
    pub relation_deltas: Vec<RelationDelta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectorLogEntry {
    pub id: i64,
    pub trigger: String,
    pub output: DirectorOutput,
    /// `llm` or `fallback` — did the plan come from the model or from the safety net.
    pub source: String,
    pub created_at: String,
}

// ───────────────────────────── Synthesized schema ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchemaAnnotation {
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchemaDoc {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub mermaid: String,
    #[serde(default)]
    pub annotations: Vec<SchemaAnnotation>,
    #[serde(default)]
    pub created_at: String,
}

// ───────────────────────────── API ─────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ChatRequest {
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomState {
    pub agents: Vec<Agent>,
    pub messages: Vec<Message>,
    pub graph: Graph,
    pub graph_mermaid: String,
    pub schema: Option<SchemaDoc>,
    pub last_director: Option<DirectorLogEntry>,
    pub mock_mode: bool,
    pub model: String,
    pub director_model: String,
    pub version: String,
    pub update: Option<crate::update::UpdateInfo>,
    /// Can this client change the key/model via `POST /api/settings`?
    pub settings_editable: bool,
    pub contracts: Vec<Contract>,
    pub relations: Vec<Relation>,
    pub memories: Vec<AgentMemory>,
    pub events: Vec<RoomEventRecord>,
    pub now: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingsView {
    pub has_key: bool,
    pub key_hint: Option<String>,
    pub model: String,
    pub director_model: String,
    pub editable: bool,
    pub config_path: String,
    pub version: String,
    pub update: Option<crate::update::UpdateInfo>,
    pub ui_embedded: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SettingsPatch {
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub clear_key: bool,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub director_model: Option<String>,
}

/// Live events pushed over SSE (`GET /api/events`) while a round is running.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum RoomEvent {
    Status { stage: String, detail: String },
    Message { message: Message },
    Director { entry: DirectorLogEntry },
    Agents { agents: Vec<Agent> },
    Graph { graph: Graph, mermaid: String },
    Schema { schema: SchemaDoc },
    Contracts { contracts: Vec<Contract> },
    Relations { relations: Vec<Relation> },
    Memories { memories: Vec<AgentMemory> },
    Done,
}
