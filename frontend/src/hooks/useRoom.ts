import { useCallback, useEffect, useRef, useState } from 'react'
import { api, ApiError, subscribeEvents } from '../api'
import type { Message, RoomEvent, RoomState } from '../types'

export type Busy = { stage: string; detail: string } | null

function mergeMessages(prev: Message[], incoming: Message[]): Message[] {
  const map = new Map<number, Message>()
  for (const m of prev) map.set(m.id, m)
  for (const m of incoming) map.set(m.id, m)
  return [...map.values()].sort((a, b) => a.id - b.id)
}

export function useRoom() {
  const [state, setState] = useState<RoomState | null>(null)
  const [busy, setBusy] = useState<Busy>(null)
  const [error, setError] = useState<string | null>(null)
  const [live, setLive] = useState(false)
  const [gate, setGate] = useState<{ code: string; message: string; bot: string | null } | null>(null)
  const inflight = useRef(false)

  const refresh = useCallback(async () => {
    try {
      const s = await api.state()
      setState(s)
      setError(null)
      setGate(null)
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) {
        let bot: string | null = null
        try {
          bot = (await api.health()).telegram.bot_username
        } catch {
          /* ignore */
        }
        setGate({ code: e.code, message: e.message, bot })
        return
      }
      setError((e as Error).message)
    }
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  useEffect(() => {
    const apply = (ev: RoomEvent) => {
      switch (ev.type) {
        case 'status':
          setBusy({ stage: ev.stage, detail: ev.detail })
          break
        case 'done':
          setBusy(null)
          break
        case 'message':
          setState((s) => (s ? { ...s, messages: mergeMessages(s.messages, [ev.message]) } : s))
          break
        case 'director':
          setState((s) => (s ? { ...s, last_director: ev.entry } : s))
          break
        case 'agents':
          setState((s) => (s ? { ...s, agents: ev.agents } : s))
          break
        case 'graph':
          setState((s) => (s ? { ...s, graph: ev.graph, graph_mermaid: ev.mermaid } : s))
          break
        case 'schema':
          setState((s) => (s ? { ...s, schema: ev.schema } : s))
          break
        case 'contracts':
          setState((s) => (s ? { ...s, contracts: ev.contracts } : s))
          break
        case 'relations':
          setState((s) => (s ? { ...s, relations: ev.relations } : s))
          break
        case 'memories':
          setState((s) => (s ? { ...s, memories: ev.memories } : s))
          break
      }
    }
    const unsub = subscribeEvents(apply, setLive)
    return unsub
  }, [])

  const run = useCallback(
    async (label: string, fn: () => Promise<RoomState>) => {
      if (inflight.current) return
      inflight.current = true
      setBusy((b) => b ?? { stage: 'request', detail: label })
      setError(null)
      try {
        const s = await fn()
        setState((prev) => (prev ? { ...s, messages: mergeMessages(prev.messages, s.messages) } : s))
      } catch (e) {
        setError((e as Error).message)
      } finally {
        inflight.current = false
        setBusy(null)
      }
    },
    [],
  )

  return {
    state,
    busy,
    error,
    gate,
    live,
    refresh,
    send: (text: string) => run('Отправляю в комнату…', () => api.chat(text)),
    tick: () => run('Автономный тик…', api.tick),
    synthesize: () => run('Собираю схему…', api.schema),
    resolveContract: (id: number, status: 'done' | 'failed' | 'cancelled', note?: string) =>
      run('Выношу вердикт…', () => api.resolveContract(id, status, note)),
    reset: () => run('Новая комната…', api.reset),
  }
}
