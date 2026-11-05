import type { GraphNode, Message, ObjectType, RoomEvent, RoomState, SettingsPatch, SettingsView } from './types'
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

export const api = {
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
