import { useEffect, useRef } from 'react'
import type { Agent, GraphNode, Message, ObjectType } from '../types'
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
  onSend: (text: string, nodeId?: string | null) => void
  onTick: () => void
  contextNode: GraphNode | null
  onClearContext: () => void
  onNodeClick: (id: string) => void
  onOpenAgent: (id: string) => void
  mood: string | null
  lastSeen: number
  streaming: { agent: string; text: string } | null
  onSeen: (messageId: number) => void
}

export function ScenePanel(p: Props) {
  const bottom = useRef<HTMLDivElement>(null)
  const lastId = p.messages.length ? p.messages[p.messages.length - 1].id : 0
  const onSeen = p.onSeen

  useEffect(() => {
    bottom.current?.scrollIntoView({ behavior: 'smooth', block: 'end' })
  }, [p.messages.length, p.busy?.detail, p.streaming?.text.length])

  // Seen = the newest message is on screen and the tab is visible.
  useEffect(() => {
    if (!lastId) return
    const t = setTimeout(() => {
      if (document.visibilityState === 'visible') onSeen(lastId)
    }, 1500)
    return () => clearTimeout(t)
  }, [lastId, onSeen])

  const firstUnread = p.messages.find((m) => m.id > p.lastSeen && m.sender_id !== 'user')
  const unreadCount = p.messages.filter((m) => m.id > p.lastSeen && m.sender_id !== 'user' && m.sender_id !== 'system').length
  const streamAgent = p.streaming ? p.agents.find((a) => a.id === p.streaming!.agent) : null

  return (
    <div className="flex h-full min-h-0 flex-col">
      <AgentStrip agents={p.agents} speaking={p.speaking} onOpen={p.onOpenAgent} />
      {p.mood && <div className="border-b border-line px-3 py-1 font-mono text-[10px] text-zinc-500">в комнате · {p.mood}</div>}
      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto px-3 py-3">
        {p.messages.map((m) => (
          <div key={m.id}>
            {firstUnread && m.id === firstUnread.id && unreadCount > 0 && (
              <div className="my-2 flex items-center gap-2 font-mono text-[10px] uppercase tracking-widest text-acid">
                <span className="h-px flex-1 bg-acid/40" />
                пока тебя не было · {unreadCount}
                <span className="h-px flex-1 bg-acid/40" />
              </div>
            )}
            <MessageBubble message={m} agents={p.agents} nodes={p.nodes} types={p.types} showEgo={p.showEgo} onNodeClick={p.onNodeClick} />
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
