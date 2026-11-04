import { useState } from 'react'
import { ChevronDown, ChevronRight, Terminal } from 'lucide-react'
import type { Agent, Message } from '../types'
import { AGENT_COLORS, AGENT_GLYPH } from '../types'

interface Props {
  message: Message
  agents: Agent[]
  showEgo: boolean
}

export function MessageBubble({ message, agents, showEgo }: Props) {
  const [open, setOpen] = useState(true)
  const agent = agents.find((a) => a.id === message.sender_id)
  const isUser = message.sender_id === 'user'
  const isSystem = message.sender_id === 'system'
  const color = agent?.color ?? AGENT_COLORS[message.sender_id] ?? '#aaa'
  const time = new Date(message.created_at).toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' })

  if (isSystem) {
    return (
      <div className="mx-auto my-1 max-w-[90%] rounded border border-dashed border-line px-3 py-1.5 text-center text-[11px] font-mono text-zinc-500">
        {message.content}
      </div>
    )
  }

  if (isUser) {
    return (
      <div className="flex justify-end">
        <div className="max-w-[88%] rounded-lg rounded-br-sm border border-zinc-600 bg-zinc-800/70 px-3 py-2 text-sm text-zinc-100">
          <div className="whitespace-pre-wrap break-words">{message.content}</div>
          <div className="mt-1 text-right text-[10px] font-mono text-zinc-500">ты · {time}</div>
        </div>
      </div>
    )
  }

  return (
    <div className="flex gap-2">
      <div
        className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-sm border text-sm font-bold"
        style={{ borderColor: color, color }}
      >
        {AGENT_GLYPH[message.sender_id] ?? '?'}
      </div>
      <div className="min-w-0 max-w-[88%] flex-1">
        <div className="mb-0.5 flex items-baseline gap-2">
          <span className="text-xs font-semibold" style={{ color }}>
            {agent?.name ?? message.sender_id}
          </span>
          <span className="text-[10px] font-mono text-zinc-600">{time}</span>
        </div>
        {showEgo && message.ego_thought && (
          <div className="mb-1 rounded border border-line bg-black/50 font-mono text-[11px]">
            <button
              onClick={() => setOpen((v) => !v)}
              className="flex w-full items-center gap-1 px-2 py-1 text-left text-zinc-500 hover:text-zinc-300"
            >
              {open ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
              <Terminal size={11} />
              <span className="tracking-widest">&lt;ego_thought&gt;</span>
            </button>
            {open && (
              <div className="whitespace-pre-wrap break-words border-t border-line px-2.5 py-1.5 leading-relaxed text-zinc-400/90">
                {message.ego_thought}
              </div>
            )}
          </div>
        )}
        <div
          className="rounded-lg rounded-tl-sm border bg-panel px-3 py-2 text-sm leading-relaxed text-zinc-100"
          style={{ borderColor: `${color}55` }}
        >
          <div className="whitespace-pre-wrap break-words">{message.content}</div>
        </div>
      </div>
    </div>
  )
}
