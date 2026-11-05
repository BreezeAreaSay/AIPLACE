import { useEffect, useRef, useState } from 'react'
import { ArrowLeft, Lock, Send } from 'lucide-react'
import type { Agent, GraphNode, Message, ObjectType } from '../types'
import { glyphOf } from '../types'
import { MessageBubble } from './MessageBubble'

interface Props {
  agent: Agent
  agents: Agent[]
  messages: Message[]
  nodes: GraphNode[]
  types: ObjectType[]
  busy: boolean
  serverTts: boolean
  onBack: () => void
  onSend: (text: string, replyTo?: number | null) => Promise<unknown>
  onLoadOlder: () => Promise<unknown>
  onReact: (id: number, emoji: string) => void
  onDelete: (id: number) => void
  onNodeClick: (id: string) => void
}

/** Private conversation with one inhabitant: nobody else hears it. */
export function DmPanel({ agent, agents, messages, nodes, types, busy, serverTts, onBack, onSend, onLoadOlder, onReact, onDelete, onNodeClick }: Props) {
  const [text, setText] = useState('')
  const [replyTo, setReplyTo] = useState<Message | null>(null)
  const bottom = useRef<HTMLDivElement>(null)
  useEffect(() => {
    bottom.current?.scrollIntoView({ block: 'end' })
  }, [messages.length])
  const dead = agent.hp <= 0 || agent.active === 0
  const submit = () => {
    const t = text.trim()
    if (!t || busy) return
    void onSend(t, replyTo?.id ?? null)
    setText('')
    setReplyTo(null)
  }
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex items-center gap-2 border-b border-line px-3 py-2">
        <button onClick={onBack} className="rounded p-1 text-zinc-500 hover:text-zinc-200" title="Назад во двор">
          <ArrowLeft size={15} />
        </button>
        <span className="flex h-7 w-7 items-center justify-center rounded-sm border text-sm font-bold" style={{ borderColor: agent.color, color: agent.color }}>
          {glyphOf(agent)}
        </span>
        <div className="min-w-0">
          <div className="truncate text-sm font-semibold text-zinc-100">{agent.name}</div>
          <div className="flex items-center gap-1 font-mono text-[10px] text-zinc-500">
            <Lock size={9} /> личная переписка · соседи не слышат
          </div>
        </div>
      </div>
      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto px-3 py-3">
        {messages.length >= 60 && (
          <button onClick={() => void onLoadOlder()} className="mx-auto block rounded border border-line px-3 py-1 font-mono text-[10px] text-zinc-500 hover:border-acid hover:text-acid">
            показать раньше
          </button>
        )}
        {messages.length === 0 && (
          <div className="mx-auto max-w-sm rounded border border-dashed border-line px-4 py-6 text-center text-[12px] text-zinc-500">
            Здесь только вы двое. {agent.name.split(' ')[0]} помнит общий двор, но то, что сказано тут, остаётся тут.
          </div>
        )}
        {messages.map((m) => (
          <div key={m.id} id={`dm-${m.id}`}>
            <MessageBubble message={m} agents={agents} nodes={nodes} types={types} showEgo={false} onNodeClick={onNodeClick} serverTts={serverTts} onReact={onReact} onReply={(msg) => setReplyTo(msg)} onDelete={onDelete} messages={messages} />
          </div>
        ))}
        {busy && (
          <div className="flex items-center gap-2 px-1 font-mono text-[11px] text-zinc-500">
            <span className="inline-block h-1.5 w-1.5 animate-pulseLine rounded-full bg-acid" />
            <span className="animate-pulseLine">{agent.name.split(' ')[0]} печатает…</span>
          </div>
        )}
        <div ref={bottom} />
      </div>
      <div className="border-t border-line bg-void px-3 pb-3 pt-2">
        {replyTo && (
          <div className="mb-2 flex items-center gap-2 rounded border border-line bg-panel px-2 py-1 font-mono text-[10px] text-zinc-400">
            <span>в ответ на:</span>
            <span className="truncate text-zinc-200">{replyTo.content}</span>
            <button onClick={() => setReplyTo(null)} className="ml-auto text-zinc-500 hover:text-zinc-200">
              ×
            </button>
          </div>
        )}
        <div className="flex items-end gap-2">
          <textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault()
                submit()
              }
            }}
            rows={1}
            disabled={dead}
            placeholder={dead ? `${agent.name.split(' ')[0]} сейчас не отвечает` : `Написать ${agent.name.split(' ')[0]} лично…`}
            className="max-h-40 min-h-[42px] flex-1 resize-y rounded border border-line bg-panel px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-600 focus:border-acid focus:outline-none disabled:opacity-50"
          />
          <button onClick={submit} disabled={!text.trim() || busy || dead} className="flex h-10 w-10 items-center justify-center rounded bg-acid text-void hover:shadow-glow disabled:opacity-40" aria-label="Отправить">
            <Send size={16} />
          </button>
        </div>
      </div>
    </div>
  )
}
