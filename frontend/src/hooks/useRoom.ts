import { useCallback, useEffect, useRef, useState } from 'react'
import { api, ApiError, subscribeEvents } from '../api'
import { registerAgents, type Message, type RoomEvent, type RoomState } from '../types'

export type Dms = Record<string, Message[]>

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
  const [streaming, setStreaming] = useState<{ agent: string; text: string } | null>(null)
  const [dms, setDms] = useState<Dms>({})
  const [dmBusy, setDmBusy] = useState<string | null>(null)
  const inflight = useRef(false)
  const seenSent = useRef(0)

  const refresh = useCallback(async () => {
    try {
      const s = await api.state()
      registerAgents(s.agents)
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
          setStreaming(null)
          if (ev.message.channel) {
            const ch = ev.message.channel
            setDms((d) => ({ ...d, [ch]: mergeMessages(d[ch] ?? [], [ev.message]) }))
          } else {
            setState((s) => (s ? { ...s, messages: mergeMessages(s.messages, [ev.message]) } : s))
          }
          break
        case 'proposals':
          setState((s) => (s ? { ...s, proposals: ev.proposals } : s))
          break
        case 'stream_start':
          setStreaming({ agent: ev.agent, text: '' })
          break
        case 'delta':
          setStreaming((st) => (st && st.agent === ev.agent ? { ...st, text: st.text + ev.text } : { agent: ev.agent, text: ev.text }))
          break
        case 'director':
          setState((s) => (s ? { ...s, last_director: ev.entry } : s))
          break
        case 'agents':
          registerAgents(ev.agents)
          setState((s) => (s ? { ...s, agents: ev.agents } : s))
          break
        case 'graph':
          setState((s) => (s ? { ...s, graph: ev.graph, graph_mermaid: ev.mermaid } : s))
          break
        case 'notes':
          setState((s) => (s ? { ...s, node_notes: ev.notes } : s))
          break
        case 'types':
          setState((s) => (s ? { ...s, types: ev.types } : s))
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
        case 'attachments':
          setState((s) => (s ? { ...s, attachments: ev.attachments } : s))
          break
        case 'message_deleted':
          setState((s) => (s ? { ...s, messages: s.messages.filter((m) => m.id !== ev.id), message_count: Math.max(0, s.message_count - 1) } : s))
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

  /** Fire-and-forget canvas edits: no busy state, errors surface in the banner. */
  const quiet = useCallback(async (fn: () => Promise<unknown>) => {
    try {
      await fn()
      setError(null)
    } catch (e) {
      setError((e as Error).message)
    }
  }, [])

  /** Tell the server the player has seen everything up to the newest message (debounced). */
  const markSeen = useCallback((messageId: number) => {
    if (messageId <= seenSent.current) return
    seenSent.current = messageId
    void api.seen(messageId).catch(() => undefined)
    setState((s) => (s ? { ...s, last_seen_message_id: Math.max(s.last_seen_message_id, messageId) } : s))
  }, [])

  return {
    state,
    busy,
    error,
    gate,
    live,
    streaming,
    markSeen,
    refresh,
    send: async (text: string, nodeId?: string | null, replyTo?: number | null) => {
      // Sending is always allowed: mid-round lines land immediately and get a follow-up round.
      setError(null)
      try {
        const s = await api.chat(text, nodeId, replyTo)
        setState((prev) => (prev ? { ...s, messages: mergeMessages(prev.messages, s.messages) } : s))
      } catch (e) {
        setError((e as Error).message)
      }
    },
    createNode: (label: string, opts?: { body?: string; type?: string; type_id?: string; props?: Record<string, unknown>; x?: number; y?: number }) =>
      run('Добавляю объект…', () => api.createNode(label, opts)),
    importMd: (files: { name: string; content: string }[]) => api.importMd(files).then((r) => (void refresh(), r)),
    uploadToNode: (id: string, files: File[]) => api.uploadToNode(id, files).then((r) => (void refresh(), r)),
    uploadFiles: (files: File[], type?: string) => api.uploadFiles(files, type).then((r) => (void refresh(), r)),
    deleteFile: (id: string) => quiet(() => api.deleteFile(id)),
    updateType: (id: string, patch: Parameters<typeof api.updateType>[1]) => quiet(() => api.updateType(id, patch)),
    deleteType: (id: string) => quiet(() => api.deleteType(id)),
    autofill: (id: string, kind?: string, query?: string) => api.autofill(id, kind, query).then((r) => (void refresh(), r)),
    githubInto: (id: string) => api.githubInto(id).then((r) => (void refresh(), r)),
    pollFeeds: () => api.pollFeeds().then((r) => (void refresh(), r)),
    loadOlder: async () => {
      const first = state?.messages[0]?.id
      if (!first) return 0
      const older = await api.olderMessages(first, 60)
      setState((s) => (s ? { ...s, messages: mergeMessages(older, s.messages) } : s))
      return older.length
    },
    loadAround: async (id: number) => {
      const around = await api.messagesAround(id, 40)
      setState((s) => (s ? { ...s, messages: mergeMessages(s.messages, around) } : s))
    },
    editMessage: (id: number, content: string) => quiet(() => api.editMessage(id, content)),
    react: (id: number, emoji: string) => quiet(() => api.react(id, emoji)),
    dms,
    dmBusy,
    loadDm: async (agent: string, older = false) => {
      const before = older ? dms[agent]?.[0]?.id : undefined
      const list = await api.dm(agent, before)
      setDms((d) => ({ ...d, [agent]: mergeMessages(list, older ? (d[agent] ?? []) : []) }))
    },
    sendDm: async (agent: string, text: string, replyTo?: number | null) => {
      setDmBusy(agent)
      setError(null)
      try {
        const pair = await api.dmSend(agent, text, replyTo)
        setDms((d) => ({ ...d, [agent]: mergeMessages(d[agent] ?? [], pair) }))
      } catch (e) {
        setError((e as Error).message)
      } finally {
        setDmBusy(null)
      }
    },
    acceptProposal: (id: number) => api.acceptProposal(id).then((r) => (void refresh(), r)),
    rejectProposal: (id: number) => api.rejectProposal(id).then((r) => (void refresh(), r)),
    deleteMessage: (id: number) => quiet(() => api.deleteMessage(id)),
    createAgent: (body: Parameters<typeof api.createAgent>[0]) => api.createAgent(body).then((a) => (registerAgents(a), setState((s) => (s ? { ...s, agents: a } : s)), a)),
    patchAgent: (id: string, body: Parameters<typeof api.patchAgent>[1]) => api.patchAgent(id, body).then((a) => (registerAgents(a), setState((s) => (s ? { ...s, agents: a } : s)), a)),
    createType: (name: string, props?: { key: string; label: string; kind: string; options?: string[] }[]) => quiet(() => api.createType(name, props)),
    patchNode: (id: string, patch: Parameters<typeof api.patchNode>[1]) => {
      // optimistic local update so the panel feels instant
      setState((s) =>
        s
          ? {
              ...s,
              graph: {
                ...s.graph,
                nodes: s.graph.nodes.map((n) =>
                  n.id === id
                    ? {
                        ...n,
                        label: patch.label ?? n.label,
                        body: patch.body ?? n.body,
                        node_type: patch.type ?? n.node_type,
                        type_id: patch.type_id ?? n.type_id,
                        props: patch.props ? { ...n.props, ...patch.props } : n.props,
                        archived: patch.archived === undefined ? n.archived : patch.archived ? 1 : 0,
                        x: patch.x ?? n.x,
                        y: patch.y ?? n.y,
                        pinned: patch.pinned === undefined ? n.pinned : patch.pinned ? 1 : 0,
                      }
                    : n,
                ),
              },
            }
          : s,
      )
      return quiet(() => api.patchNode(id, patch))
    },
    deleteNode: (id: string) => quiet(() => api.deleteNode(id)),
    createEdge: (source: string, target: string, label?: string, kind?: string) => quiet(() => api.createEdge(source, target, label, kind)),
    deleteEdge: (id: number) => quiet(() => api.deleteEdge(id)),
    addNote: (id: string, text: string) => quiet(() => api.addNote(id, text)),
    askNode: (id: string, mode: 'discuss' | 'develop' | 'dispute', message?: string) =>
      run('Триада смотрит на узел…', () => api.askNode(id, mode, message)),
    tick: () => run('Автономный тик…', api.tick),
    synthesize: () => run('Собираю схему…', api.schema),
    resolveContract: (id: number, status: 'done' | 'failed' | 'cancelled', note?: string) =>
      run('Выношу вердикт…', () => api.resolveContract(id, status, note)),
    reset: () => run('Новая комната…', api.reset),
  }
}
