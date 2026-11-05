import type { RoomEvent, RoomState, SettingsPatch, SettingsView } from './types'

// Single switch between local dev (proxy) and a remote VPS behind Telegram Mini Apps.
export const API_BASE: string = (import.meta.env.VITE_API_URL as string | undefined)?.replace(/\/$/, '') ?? ''

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...init,
  })
  const text = await res.text()
  let data: unknown = null
  try {
    data = text ? JSON.parse(text) : null
  } catch {
    /* non-JSON error body */
  }
  if (!res.ok) {
    const msg = (data as { error?: string } | null)?.error ?? `${res.status} ${res.statusText}`
    throw new Error(msg)
  }
  return data as T
}

export const api = {
  state: () => request<RoomState>('/api/state'),
  chat: (message: string) => request<RoomState>('/api/chat', { method: 'POST', body: JSON.stringify({ message }) }),
  tick: () => request<RoomState>('/api/tick', { method: 'POST' }),
  schema: () => request<RoomState>('/api/schema', { method: 'POST' }),
  reset: () => request<RoomState>('/api/reset', { method: 'POST' }),
  resolveContract: (id: number, status: 'done' | 'failed' | 'cancelled', note?: string) =>
    request<RoomState>(`/api/contracts/${id}/resolve`, { method: 'POST', body: JSON.stringify({ status, note }) }),
  settings: () => request<SettingsView>('/api/settings'),
  saveSettings: (patch: SettingsPatch) => request<SettingsView>('/api/settings', { method: 'POST', body: JSON.stringify(patch) }),
}

/** Subscribe to live room events (SSE). Returns an unsubscribe function. */
export function subscribeEvents(onEvent: (ev: RoomEvent) => void, onStatus?: (ok: boolean) => void): () => void {
  const es = new EventSource(`${API_BASE}/api/events`)
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
