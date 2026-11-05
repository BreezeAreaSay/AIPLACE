import { useEffect, useRef } from 'react'
import type { Agent, GraphNode, Message, ObjectType } from '../types'
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
}

export function ScenePanel(p: Props) {
  const bottom = useRef<HTMLDivElement>(null)
  useEffect(() => {
    bottom.current?.scrollIntoView({ behavior: 'smooth', block: 'end' })
  }, [p.messages.length, p.busy?.detail])
  return (
    <div className="flex h-full min-h-0 flex-col">
      <AgentStrip agents={p.agents} speaking={p.speaking} onOpen={p.onOpenAgent} />
      {p.mood && <div className="border-b border-line px-3 py-1 font-mono text-[10px] text-zinc-500">в комнате · {p.mood}</div>}
      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto px-3 py-3">
            {p.messages.map((m) => (
              <MessageBubble key={m.id} message={m} agents={p.agents} nodes={p.nodes} types={p.types} showEgo={p.showEgo} onNodeClick={p.onNodeClick} />
            ))}
            {p.busy && (
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
