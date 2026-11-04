import { useEffect, useRef } from 'react'
import type { Agent, Message } from '../types'
import { MessageBubble } from './MessageBubble'
import type { Busy } from '../hooks/useRoom'

interface Props {
  messages: Message[]
  agents: Agent[]
  showEgo: boolean
  busy: Busy
}

export function ChatStream({ messages, agents, showEgo, busy }: Props) {
  const bottom = useRef<HTMLDivElement>(null)
  useEffect(() => {
    bottom.current?.scrollIntoView({ behavior: 'smooth', block: 'end' })
  }, [messages.length, busy?.detail])
  return (
    <div className="flex-1 space-y-3 overflow-y-auto px-3 py-3">
      {messages.map((m) => (
        <MessageBubble key={m.id} message={m} agents={agents} showEgo={showEgo} />
      ))}
      {busy && (
        <div className="flex items-center gap-2 px-1 font-mono text-[11px] text-zinc-500">
          <span className="inline-block h-1.5 w-1.5 animate-pulseLine rounded-full bg-acid" />
          <span className="animate-pulseLine">{busy.detail}</span>
        </div>
      )}
      <div ref={bottom} />
    </div>
  )
}
