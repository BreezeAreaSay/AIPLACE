export type AgentId = 'physicist' | 'lit_teacher' | 'graffiti'

export interface Agent {
  id: AgentId | string
  name: string
  archetype: string
  system_core: string
  hp: number
  dopamine: number
  current_agenda: string
  respect_user: number
  color: string
}

export interface Message {
  id: number
  sender_id: string
  ego_thought: string | null
  content: string
  created_at: string
}

export interface GraphNode {
  id: string
  label: string
  node_type: 'concept' | 'task' | 'conflict' | 'insight' | string
  created_by: string
  created_at: string
}

export interface GraphEdge {
  id: number
  source_id: string
  target_id: string
  relation_label: string
  created_by: string
}

export interface Graph {
  nodes: GraphNode[]
  edges: GraphEdge[]
}

export interface StatDelta {
  hp: number
  dopamine: number
  respect_user: number
  reason?: string | null
}

export interface DirectorOutput {
  room_mood?: string | null
  director_note?: string | null
  next_speakers: string[]
  updated_agendas: Record<string, string>
  stat_deltas: Record<string, StatDelta>
  graph_mutations: {
    nodes: { id: string; label: string; type: string; author: string }[]
    edges: { from: string; to: string; label: string; author?: string | null }[]
  }
}

export interface DirectorLogEntry {
  id: number
  trigger: 'user_message' | 'tick' | string
  output: DirectorOutput
  source: 'llm' | 'fallback' | string
  created_at: string
}

export interface SchemaDoc {
  title: string
  mermaid: string
  annotations: { author: string; text: string }[]
  created_at: string
}

export interface RoomState {
  agents: Agent[]
  messages: Message[]
  graph: Graph
  graph_mermaid: string
  schema: SchemaDoc | null
  last_director: DirectorLogEntry | null
  mock_mode: boolean
  model: string
  director_model: string
}

export type RoomEvent =
  | { type: 'status'; stage: string; detail: string }
  | { type: 'message'; message: Message }
  | { type: 'director'; entry: DirectorLogEntry }
  | { type: 'agents'; agents: Agent[] }
  | { type: 'graph'; graph: Graph; mermaid: string }
  | { type: 'schema'; schema: SchemaDoc }
  | { type: 'done' }

export const AGENT_COLORS: Record<string, string> = {
  physicist: '#00F0FF',
  lit_teacher: '#FF0055',
  graffiti: '#CCFF00',
  user: '#ffffff',
  system: '#8b8d9a',
}

export const AGENT_GLYPH: Record<string, string> = {
  physicist: 'Σ',
  lit_teacher: 'Ѣ',
  graffiti: '☣',
}
