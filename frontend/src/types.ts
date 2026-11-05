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
  inner_state: string
}

export interface Message {
  id: number
  sender_id: string
  ego_thought: string | null
  content: string
  created_at: string
  addressee: string | null
  move: string | null
  node_id: string | null
  objects: string[]
  actions: { op: string; object_id: string; text: string }[]
}

export interface PropSchema {
  key: string
  label: string
  kind: 'text' | 'date' | 'number' | 'select' | 'checkbox' | 'url' | 'relation' | string
  options?: string[]
  to?: string
}

export interface ObjectType {
  id: string
  name: string
  icon: string
  color: string
  schema: { props: PropSchema[] }
  builtin: number
  created_by: string
  created_at: string
}

export interface GraphNode {
  id: string
  label: string
  node_type: 'concept' | 'task' | 'conflict' | 'insight' | string
  created_by: string
  created_at: string
  body: string
  x: number | null
  y: number | null
  pinned: number
  updated_at: string
  type_id: string
  props: Record<string, unknown>
  archived: number
}

export interface NodeNote {
  id: number
  node_id: string
  author: string
  text: string
  created_at: string
}

export interface Beat {
  speaker: string
  addressee: string | null
  move: string | null
  intent: string | null
  delivery: string | null
}

export interface GraphEdge {
  id: number
  source_id: string
  target_id: string
  relation_label: string
  created_by: string
  kind: string
  confidence: number
}

export const RELATION_KIND_LABEL: Record<string, string> = {
  related: 'связано',
  belongs_to: 'часть',
  about: 'о',
  blocks: 'блокирует',
  made_by: 'сделано',
  assigned_to: 'исполнитель',
  contradicts: 'противоречит',
  mentions: 'упоминает',
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

export interface Contract {
  id: number
  title: string
  details: string | null
  deadline: string | null
  status: 'open' | 'done' | 'failed' | 'cancelled' | string
  guarantor_id: string | null
  stake: number
  created_at: string
  resolved_at: string | null
  resolution_note: string | null
}

export interface Relation {
  a_id: string
  b_id: string
  affinity: number
  label: string
}

export interface AgentMemory {
  agent_id: string
  summary: string
  covered_until: number
  updated_at: string
}

export interface RoomEventRecord {
  id: number
  kind: string
  payload: string
  narrated: number
  created_at: string
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
  new_contracts: { title: string; deadline?: string | null; guarantor: string; stake?: number | null; why?: string | null }[]
  contract_updates: { id: number | string; status: string; note?: string | null }[]
  relation_deltas: { a: string; b: string; delta: number; label?: string | null }[]
  scene: Beat[]
  inner_states: Record<string, string>
  node_updates: { id: string; label?: string | null; body?: string | null; type?: string | null }[]
  stages: { stage: string; ok: boolean; ms: number; error?: string | null }[]
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
  types: ObjectType[]
  node_notes: NodeNote[]
  graph_mermaid: string
  schema: SchemaDoc | null
  last_director: DirectorLogEntry | null
  mock_mode: boolean
  model: string
  director_model: string
  version: string
  update: UpdateInfo | null
  settings_editable: boolean
  contracts: Contract[]
  relations: Relation[]
  memories: AgentMemory[]
  events: RoomEventRecord[]
  now: string
  room_id: string
  last_seen_message_id: number
  usage: { calls_today: number; daily_budget: number }
  heartbeat_minutes: number
}

export interface UpdateInfo {
  current: string
  latest: string
  url: string
}

export interface SettingsView {
  has_key: boolean
  key_hint: string | null
  model: string
  director_model: string
  editable: boolean
  config_path: string
  version: string
  update: UpdateInfo | null
  ui_embedded: boolean
  director_mode: string
  agent_mode: string
}

export interface SettingsPatch {
  api_key?: string
  clear_key?: boolean
  model?: string
  director_model?: string
}

export type RoomEvent =
  | { type: 'status'; stage: string; detail: string }
  | { type: 'message'; message: Message }
  | { type: 'director'; entry: DirectorLogEntry }
  | { type: 'agents'; agents: Agent[] }
  | { type: 'graph'; graph: Graph; mermaid: string }
  | { type: 'notes'; notes: NodeNote[] }
  | { type: 'types'; types: ObjectType[] }
  | { type: 'stream_start'; agent: string }
  | { type: 'delta'; agent: string; text: string }
  | { type: 'schema'; schema: SchemaDoc }
  | { type: 'contracts'; contracts: Contract[] }
  | { type: 'relations'; relations: Relation[] }
  | { type: 'memories'; memories: AgentMemory[] }
  | { type: 'done' }

export const AGENT_COLORS: Record<string, string> = {
  physicist: '#00F0FF',
  lit_teacher: '#FF0055',
  graffiti: '#CCFF00',
  user: '#ffffff',
  system: '#8b8d9a',
}

export const NODE_TYPE_LABEL: Record<string, string> = {
  concept: 'концепт',
  task: 'задача',
  conflict: 'конфликт',
  insight: 'инсайт',
}

export const NODE_TYPE_GLYPH: Record<string, string> = {
  concept: '◇',
  task: '☐',
  conflict: '⚡',
  insight: '✦',
}

export const AGENT_GLYPH: Record<string, string> = {
  physicist: 'Σ',
  lit_teacher: 'Ѣ',
  graffiti: '☣',
}
