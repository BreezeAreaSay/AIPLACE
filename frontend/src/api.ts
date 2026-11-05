import type { Agent, Attachment, CallLog, GraphNode, Message, ObjectType, RolePreset, RoomEvent, RoomState, SettingsPatch, SettingsView, SourceUpdate, UpdateStatus } from './types'
import { initData } from './telegram'

// Single switch between local dev (proxy) and a remote VPS behind Telegram Mini Apps.
export const API_BASE: string = (import.meta.env.VITE_API_URL as string | undefined)?.replace(/\/$/, '') ?? ''

export class ApiError extends Error {
  code: string
  status: number
  constructor(message: string, code: string, status: number) {
    super(message)
    this.code = code
    this.status = status
  }
}

function authHeaders(): Record<string, string> {
  const d = initData()
  return d ? { Authorization: `tma ${d}` } : {}
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    ...init,
    headers: { 'Content-Type': 'application/json', ...authHeaders(), ...(init?.headers ?? {}) },
  })
  const text = await res.text()
  let data: unknown = null
  try {
    data = text ? JSON.parse(text) : null
  } catch {
    /* non-JSON error body */
  }
  if (!res.ok) {
    const body = data as { error?: string; code?: string } | null
    throw new ApiError(body?.error ?? `${res.status} ${res.statusText}`, body?.code ?? 'http_error', res.status)
  }
  return data as T
}

export interface Health {
  ok: boolean
  version: string
  mock_mode: boolean
  telegram: { enabled: boolean; bot_username: string | null; anon_rooms: boolean }
}

export interface ImportReport {
  imported: number
  updated: number
  linked: number
  ids: string[]
}

export interface NodePatch {
  label?: string
  body?: string
  type?: string
  type_id?: string
  props?: Record<string, unknown>
  archived?: boolean
  x?: number
  y?: number
  pinned?: boolean
}

async function upload<T>(path: string, files: File[], fields: Record<string, string> = {}): Promise<T> {
  const fd = new FormData()
  for (const [k, v] of Object.entries(fields)) fd.append(k, v)
  for (const f of files) fd.append('file', f, f.name)
  const res = await fetch(`${API_BASE}${path}`, { method: 'POST', body: fd, headers: authHeaders() })
  const text = await res.text()
  let data: unknown = null
  try {
    data = text ? JSON.parse(text) : null
  } catch {
    /* ignore */
  }
  if (!res.ok) {
    const body = data as { error?: string } | null
    throw new ApiError(body?.error ?? `${res.status} ${res.statusText}`, 'http_error', res.status)
  }
  return data as T
}

export interface LookupCard {
  kind: string
  title: string
  props: Record<string, unknown>
  body: string
  image_url: string | null
  url: string | null
  source: string
}

export interface OrganismState {
  health: {
    objects: number
    archived: number
    orphans: number
    stale: number
    undigested_files: number
    open_tasks: number
    overdue_tasks: number
    unread_bookmarks: number
    grown_7d: number
    touched_by_agents_7d: number
    score: number
    today_diary: string | null
  }
  needs_care: { kind: string; node_id: string; reason: string }[]
  recent: { at: string; agent: string; op: string; object_id: string; text: string; message_id: number }[]
}

export const api = {
  organism: () => request<OrganismState>('/api/organism'),
  tend: (nodeId?: string, agentId?: string) => request<Record<string, unknown>>('/api/organism/tend', { method: 'POST', body: JSON.stringify({ node_id: nodeId, agent_id: agentId }) }),
  chronicle: () => request<{ id: string | null }>('/api/organism/chronicle', { method: 'POST', body: '{}' }),
  uploadToNode: (id: string, files: File[]) => upload<Attachment[]>(`/api/nodes/${encodeURIComponent(id)}/files`, files),
  uploadFiles: (files: File[], type?: string) => upload<{ ids: string[]; attachments: Attachment[] }>('/api/files', files, type ? { type } : {}),
  fileUrl: (id: string) => `${API_BASE}/api/files/${encodeURIComponent(id)}`,
  fileText: (id: string) => request<{ id: string; name: string; chars: number; text: string }>(`/api/files/${encodeURIComponent(id)}/text`),
  deleteFile: (id: string) => request<void>(`/api/files/${encodeURIComponent(id)}`, { method: 'DELETE' }),
  updateType: (id: string, patch: { name?: string; icon?: string; color?: string; schema?: { props: unknown[]; template?: string } }) =>
    request<ObjectType[]>(`/api/types/${encodeURIComponent(id)}`, { method: 'PATCH', body: JSON.stringify(patch) }),
  deleteType: (id: string) => request<ObjectType[]>(`/api/types/${encodeURIComponent(id)}`, { method: 'DELETE' }),
  lookup: (kind: string, q: string) => request<{ card: LookupCard | null }>(`/api/lookup?kind=${encodeURIComponent(kind)}&q=${encodeURIComponent(q)}`),
  autofill: (id: string, kind?: string, query?: string) =>
    request<{ found: boolean; card?: LookupCard; filled?: string[] }>(`/api/nodes/${encodeURIComponent(id)}/autofill`, { method: 'POST', body: JSON.stringify({ kind, query }) }),
  githubInto: (id: string) => request<{ activity: string }>(`/api/nodes/${encodeURIComponent(id)}/github`, { method: 'POST', body: '{}' }),
  weather: () => request<{ configured: boolean; line?: string }>('/api/weather'),
  calendarUrl: () => `${window.location.origin}${API_BASE}/api/calendar.ics`,
  calendar: (days = 7) => request<{ events: { when: string; all_day: boolean; title: string; place: string; source: string }[]; digest: string }>(`/api/calendar?days=${days}`),
  tts: async (text: string, agent: string) => {
    const res = await fetch(`${API_BASE}/api/tts`, { method: 'POST', headers: { 'Content-Type': 'application/json', ...authHeaders() }, body: JSON.stringify({ text, agent }) })
    if (!res.ok) throw new ApiError((await res.text()) || 'tts', 'http_error', res.status)
    return res.blob()
  },
  stt: (blob: Blob) => upload<{ text: string }>('/api/stt', [new File([blob], 'voice.webm', { type: blob.type || 'audio/webm' })]),
  pollFeeds: () => request<{ created: number }>('/api/feeds/poll', { method: 'POST', body: '{}' }),
  roles: () => request<RolePreset[]>('/api/roles'),
  olderMessages: (before: number, limit = 60) => request<Message[]>(`/api/messages?before=${before}&limit=${limit}`),
  messagesAround: (id: number, limit = 40) => request<Message[]>(`/api/messages?around=${id}&limit=${limit}`),
  searchMessages: (q: string) => request<Message[]>(`/api/messages/search?q=${encodeURIComponent(q)}`),
  editMessage: (id: number, content: string) => request<Message>(`/api/messages/${id}`, { method: 'PATCH', body: JSON.stringify({ content }) }),
  deleteMessage: (id: number) => request<void>(`/api/messages/${id}`, { method: 'DELETE' }),
  llmLog: () => request<CallLog[]>('/api/llm/log'),
  backupUrl: () => `${API_BASE}/api/backup.zip`,
  restore: (file: File) => upload<{ staged: boolean; manifest: Record<string, unknown>; message: string }>('/api/restore', [file]),
  restoreCancel: () => request<void>('/api/restore', { method: 'DELETE' }),
  updateCheck: () => request<SourceUpdate>('/api/update/check', { method: 'POST', body: '{}' }),
  updateRun: () => request<{ ok: boolean; log: string; message: string }>('/api/update/run', { method: 'POST', body: '{}' }),
  updateStatus: () => request<UpdateStatus>('/api/update/status'),
  updateAck: () => request<void>('/api/update/status', { method: 'DELETE' }),
  createAgent: (body: { role?: string; name?: string; archetype?: string; system_core?: string; color?: string; glyph?: string; model?: string; voice?: string; agenda?: string }) =>
    request<Agent[]>('/api/agents', { method: 'POST', body: JSON.stringify(body) }),
  patchAgent: (id: string, body: { name?: string; archetype?: string; system_core?: string; color?: string; glyph?: string; model?: string; voice?: string; agenda?: string; active?: boolean; revive?: boolean }) =>
    request<Agent[]>(`/api/agents/${encodeURIComponent(id)}`, { method: 'PATCH', body: JSON.stringify(body) }),
  health: () => request<Health>('/api/health'),
  state: () => request<RoomState>('/api/state'),
  chat: (message: string, nodeId?: string | null) =>
    request<RoomState>('/api/chat', { method: 'POST', body: JSON.stringify({ message, node_id: nodeId ?? null }) }),
  createNode: (label: string, opts: { body?: string; type?: string; type_id?: string; props?: Record<string, unknown>; x?: number; y?: number } = {}) =>
    request<RoomState>('/api/nodes', { method: 'POST', body: JSON.stringify({ label, ...opts }) }),
  types: () => request<ObjectType[]>('/api/types'),
  createType: (name: string, props: { key: string; label: string; kind: string; options?: string[] }[] = []) =>
    request<ObjectType[]>('/api/types', { method: 'POST', body: JSON.stringify({ name, props }) }),
  backlinks: (id: string) => request<Message[]>(`/api/nodes/${encodeURIComponent(id)}/backlinks`),
  search: (q: string, type?: string) => request<GraphNode[]>(`/api/search?q=${encodeURIComponent(q)}${type ? `&type=${encodeURIComponent(type)}` : ''}`),
  patchNode: (id: string, patch: NodePatch) =>
    request<void>(`/api/nodes/${encodeURIComponent(id)}`, { method: 'PATCH', body: JSON.stringify(patch) }),
  deleteNode: (id: string) => request<void>(`/api/nodes/${encodeURIComponent(id)}`, { method: 'DELETE' }),
  createEdge: (source: string, target: string, label?: string, kind?: string) =>
    request<void>('/api/edges', { method: 'POST', body: JSON.stringify({ source, target, label, kind }) }),
  deleteEdge: (id: number) => request<void>(`/api/edges/${id}`, { method: 'DELETE' }),
  askNode: (id: string, mode: 'discuss' | 'develop' | 'dispute', message?: string) =>
    request<RoomState>(`/api/nodes/${encodeURIComponent(id)}/ask`, { method: 'POST', body: JSON.stringify({ mode, message }) }),
  addNote: (id: string, text: string) =>
    request<void>(`/api/nodes/${encodeURIComponent(id)}/notes`, { method: 'POST', body: JSON.stringify({ text }) }),
  exportUrl: () => `${API_BASE}/api/export.md`,
  nodeExportUrl: (id: string) => `${API_BASE}/api/nodes/${encodeURIComponent(id)}/export.md`,
  importMd: (files: { name: string; content: string }[]) =>
    request<ImportReport>('/api/import', { method: 'POST', body: JSON.stringify({ files }) }),
  tick: () => request<RoomState>('/api/tick', { method: 'POST' }),
  schema: () => request<RoomState>('/api/schema', { method: 'POST' }),
  reset: () => request<RoomState>('/api/reset', { method: 'POST' }),
  resolveContract: (id: number, status: 'done' | 'failed' | 'cancelled', note?: string) =>
    request<RoomState>(`/api/contracts/${id}/resolve`, { method: 'POST', body: JSON.stringify({ status, note }) }),
  seen: (messageId: number) => request<void>('/api/seen', { method: 'POST', body: JSON.stringify({ message_id: messageId }) }),
  settings: () => request<SettingsView>('/api/settings'),
  saveSettings: (patch: SettingsPatch) => request<SettingsView>('/api/settings', { method: 'POST', body: JSON.stringify(patch) }),
}

/** Subscribe to live room events (SSE). Returns an unsubscribe function. */
export function subscribeEvents(onEvent: (ev: RoomEvent) => void, onStatus?: (ok: boolean) => void): () => void {
  const d = initData()
  const es = new EventSource(`${API_BASE}/api/events${d ? `?tma=${encodeURIComponent(d)}` : ''}`)
  es.onopen = () => onStatus?.(true)
  es.onerror = () => onStatus?.(false)
  es.onmessage = (e) => {
    try {
      onEvent(JSON.parse(e.data) as RoomEvent)
    } catch {
      /* keep-alive or malformed frame */
    }
  }
  return () => es.close()
}
