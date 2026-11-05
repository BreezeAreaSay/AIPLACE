import { useEffect, useRef, useState } from 'react'
import { Search, X } from 'lucide-react'
import { api } from '../api'
import type { Agent, Features, GraphNode, Message, ObjectType, Proposal } from '../types'
import { AGENT_COLORS, AGENT_GLYPH } from '../types'
import { AgentStrip } from './AgentStrip'
import { MessageBubble } from './MessageBubble'
import { Composer } from './Composer'
import type { Busy } from '../hooks/useRoom'

interface Props {
  agents: Agent[]
  messages: Message[]
  nodes: GraphNode[]
  types: ObjectType[]
  busy: Busy
  speaking: string | null
  showEgo: boolean
  onToggleEgo: () => void
  disabled: boolean
  onSend: (text: string, nodeId?: string | null, replyTo?: number | null) => void
  onTick: () => void
  contextNode: GraphNode | null
  onClearContext: () => void
  onNodeClick: (id: string) => void
  onOpenAgent: (id: string) => void
  mood: string | null
  lastSeen: number
  streaming: { agent: string; text: string } | null
  onSeen: (messageId: number) => void
  onCapture: (label: string, typeId: string, body: string) => void
  onSearch: (q: string) => void
  features: Features
  onAttach: (files: File[]) => Promise<string[]>
  world: string | null
  onYard: () => void
  messageCount: number
  onLoadOlder: () => Promise<number>
  onLoadAround: (id: number) => Promise<void>
  onEditMessage: (id: number, content: string) => void
  onDeleteMessage: (id: number) => void
  onReact: (id: number, emoji: string) => void
  onDm: (agentId: string) => void
  proposals: Proposal[]
  onAcceptProposal: (id: number) => Promise<unknown>
  onRejectProposal: (id: number) => Promise<unknown>
}

export function ScenePanel(p: Props) {
  const bottom = useRef<HTMLDivElement>(null)
  const lastId = p.messages.length ? p.messages[p.messages.length - 1].id : 0
  const onSeen = p.onSeen

  const lastIdRef = useRef(0)
  useEffect(() => {
    // follow the bottom only when something new arrived at the end (not when older pages load)
    if (lastId !== lastIdRef.current || p.streaming) {
      lastIdRef.current = lastId
      bottom.current?.scrollIntoView({ behavior: 'smooth', block: 'end' })
    }
  }, [lastId, p.busy?.detail, p.streaming])

  // Seen = the newest message is on screen and the tab is visible.
  useEffect(() => {
    if (!lastId) return
    const t = setTimeout(() => {
      if (document.visibilityState === 'visible') onSeen(lastId)
    }, 1500)
    return () => clearTimeout(t)
  }, [lastId, onSeen])

  const [replyTo, setReplyTo] = useState<{ id: number; who: string; text: string } | null>(null)
  const [searchOpen, setSearchOpen] = useState(false)
  const [q, setQ] = useState('')
  const [hits, setHits] = useState<Message[]>([])
  const [jump, setJump] = useState<number | null>(null)
  const [loadingOlder, setLoadingOlder] = useState(false)
  const scroller = useRef<HTMLDivElement>(null)
  const hasOlder = p.messages.length > 0 && p.messages.length < p.messageCount

  useEffect(() => {
    if (!searchOpen || q.trim().length < 2) {
      setHits([])
      return
    }
    const t = setTimeout(() => void api.searchMessages(q).then(setHits).catch(() => setHits([])), 250)
    return () => clearTimeout(t)
  }, [q, searchOpen])

  const goTo = async (id: number) => {
    if (!p.messages.some((m) => m.id === id)) await p.onLoadAround(id)
    setJump(id)
    setSearchOpen(false)
    setTimeout(() => {
      document.getElementById(`msg-${id}`)?.scrollIntoView({ block: 'center' })
      setTimeout(() => setJump(null), 2500)
    }, 80)
  }

  const loadOlder = async () => {
    const el = scroller.current
    const before = el?.scrollHeight ?? 0
    setLoadingOlder(true)
    try {
      await p.onLoadOlder()
    } finally {
      setLoadingOlder(false)
    }
    requestAnimationFrame(() => {
      if (el) el.scrollTop += el.scrollHeight - before
    })
  }

  const firstUnread = p.messages.find((m) => m.id > p.lastSeen && m.sender_id !== 'user')
  const unreadCount = p.messages.filter((m) => m.id > p.lastSeen && m.sender_id !== 'user' && m.sender_id !== 'system').length
  const streamAgent = p.streaming ? p.agents.find((a) => a.id === p.streaming!.agent) : null

  return (
    <div className="flex h-full min-h-0 flex-col">
      <AgentStrip agents={p.agents} speaking={p.speaking} onOpen={p.onOpenAgent} onYard={p.onYard} />
      <div className="flex flex-wrap items-center gap-x-3 border-b border-line px-3 py-1 font-mono text-[10px] text-zinc-500">
        {p.mood && <span>в комнате · {p.mood}</span>}
        {p.world && <span className="text-zinc-600">{p.world}</span>}
        <button onClick={() => setSearchOpen((v) => !v)} className={`ml-auto flex items-center gap-1 hover:text-zinc-200 ${searchOpen ? 'text-acid' : ''}`} title="Поиск по истории чата">
          <Search size={11} /> поиск · {p.messageCount}
        </button>
      </div>
      {searchOpen && (
        <div className="relative border-b border-line bg-panel/60 px-3 py-1.5">
          <div className="flex items-center gap-2">
            <Search size={12} className="text-zinc-500" />
            <input autoFocus value={q} onChange={(e) => setQ(e.target.value)} onKeyDown={(e) => e.key === 'Escape' && setSearchOpen(false)} placeholder="слова из реплик или мыслей…" className="flex-1 bg-transparent text-xs text-zinc-100 placeholder:text-zinc-600 focus:outline-none" />
            <button onClick={() => setSearchOpen(false)} className="text-zinc-500 hover:text-zinc-200" aria-label="Закрыть поиск">
              <X size={12} />
            </button>
          </div>
          {q.trim().length >= 2 && (
            <div className="mt-1 max-h-56 overflow-y-auto rounded border border-line bg-void">
              {hits.length === 0 && <div className="px-2 py-1.5 font-mono text-[10px] text-zinc-600">ничего</div>}
              {[...hits].reverse().map((m) => {
                const who = m.sender_id === 'user' ? 'ты' : p.agents.find((a) => a.id === m.sender_id)?.name.split(' ')[0] ?? m.sender_id
                return (
                  <button key={m.id} onClick={() => void goTo(m.id)} className="flex w-full items-baseline gap-2 border-b border-line/50 px-2 py-1 text-left text-[11px] hover:bg-zinc-900">
                    <span className="shrink-0 font-mono text-[10px]" style={{ color: AGENT_COLORS[m.sender_id] ?? '#888' }}>
                      {who} · {new Date(m.created_at).toLocaleDateString('ru-RU', { day: '2-digit', month: '2-digit' })}
                    </span>
                    <span className="truncate text-zinc-300">{m.content}</span>
                  </button>
                )
              })}
            </div>
          )}
        </div>
      )}
      <div ref={scroller} className="min-h-0 flex-1 space-y-3 overflow-y-auto px-3 py-3">
        {hasOlder && (
          <button onClick={() => void loadOlder()} disabled={loadingOlder} className="mx-auto block rounded border border-line px-3 py-1 font-mono text-[10px] text-zinc-500 hover:border-acid hover:text-acid disabled:opacity-40">
            {loadingOlder ? '…' : `показать раньше · ещё ${p.messageCount - p.messages.length}`}
          </button>
        )}
        {p.messages.map((m) => (
          <div key={m.id} id={`msg-${m.id}`}>
            {firstUnread && m.id === firstUnread.id && unreadCount > 0 && (
              <div className="my-2 flex items-center gap-2 font-mono text-[10px] uppercase tracking-widest text-acid">
                <span className="h-px flex-1 bg-acid/40" />
                пока тебя не было · {unreadCount}
                <span className="h-px flex-1 bg-acid/40" />
              </div>
            )}
            <MessageBubble message={m} agents={p.agents} nodes={p.nodes} types={p.types} showEgo={p.showEgo} onNodeClick={p.onNodeClick} serverTts={p.features.tts} onEdit={p.onEditMessage} onDelete={p.onDeleteMessage} highlight={jump === m.id} onReact={p.onReact} onReply={(msg) => setReplyTo({ id: msg.id, who: msg.sender_id === 'user' ? 'ты' : p.agents.find((a) => a.id === msg.sender_id)?.name.split(' ')[0] ?? msg.sender_id, text: msg.content.slice(0, 120) })} onDm={p.onDm} messages={p.messages} proposals={p.proposals} onAcceptProposal={p.onAcceptProposal} onRejectProposal={p.onRejectProposal} />
          </div>
        ))}
        {p.streaming && streamAgent && (
          <div className="flex gap-2">
            <div className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-sm border text-sm font-bold" style={{ borderColor: streamAgent.color, color: streamAgent.color }}>
              {AGENT_GLYPH[streamAgent.id] ?? '?'}
            </div>
            <div className="min-w-0 max-w-[90%] flex-1">
              <div className="mb-0.5 text-xs font-semibold" style={{ color: AGENT_COLORS[streamAgent.id] }}>
                {streamAgent.name.split(' (')[0]} <span className="font-mono text-[10px] font-normal text-zinc-500">печатает…</span>
              </div>
              <div className="rounded-lg rounded-tl-sm border bg-panel px-3 py-2 text-sm leading-relaxed text-zinc-100" style={{ borderColor: `${streamAgent.color}55` }}>
                <span className="whitespace-pre-wrap break-words">{p.streaming.text.replace(/^<ego_thought>[\s\S]*?(<\/ego_thought>|$)/, '').replace(/<note[\s\S]*$/, '')}</span>
                <span className="ml-0.5 inline-block h-4 w-1.5 animate-pulseLine bg-acid align-middle" />
              </div>
            </div>
          </div>
        )}
        {p.busy && !p.streaming && (
          <div className="flex items-center gap-2 px-1 font-mono text-[11px] text-zinc-500">
            <span className="inline-block h-1.5 w-1.5 animate-pulseLine rounded-full bg-acid" />
            <span className="animate-pulseLine">{p.busy.detail}</span>
          </div>
        )}
        <div ref={bottom} />
      </div>
      <Composer
        disabled={p.disabled}
        types={p.types}
        onCapture={p.onCapture}
        onSearch={p.onSearch}
        features={p.features}
        onAttach={p.onAttach}
        replyTo={replyTo}
        onClearReply={() => setReplyTo(null)}
        onSend={p.onSend}
        onTick={p.onTick}
        showEgo={p.showEgo}
        onToggleEgo={p.onToggleEgo}
        contextNode={p.contextNode}
        onClearContext={p.onClearContext}
      />
    </div>
  )
}
