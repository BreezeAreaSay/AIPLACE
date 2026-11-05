import { useState } from 'react'
import { ChevronDown, ChevronRight, Terminal } from 'lucide-react'
import type { Agent, GraphNode, Message, ObjectType } from '../types'
import { AGENT_COLORS, AGENT_GLYPH } from '../types'
import { Markdown } from './Markdown'

interface Props {
  message: Message
  agents: Agent[]
  nodes: GraphNode[]
  types?: ObjectType[]
  showEgo: boolean
  onNodeClick?: (id: string) => void
}

export function MessageBubble({ message, agents, nodes, types = [], showEgo, onNodeClick }: Props) {
  const [open, setOpen] = useState(false)
  const agent = agents.find((a) => a.id === message.sender_id)
  const isUser = message.sender_id === 'user'
  const isSystem = message.sender_id === 'system'
  const color = agent?.color ?? AGENT_COLORS[message.sender_id] ?? '#aaa'
  const time = new Date(message.created_at).toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' })
  const node = message.node_id ? nodes.find((n) => n.id === message.node_id) : undefined
  const silent = /^\*[^*]+\*$/.test(message.content.trim())

  const mentioned = (message.objects ?? []).map((id) => nodes.find((n) => n.id === id)).filter((n): n is GraphNode => !!n && n.id !== node?.id)
  const chip = (n: GraphNode, strong = false) => {
    const t = types.find((x) => x.id === n.type_id)
    return (
      <button
        key={n.id}
        onClick={() => onNodeClick?.(n.id)}
        className={`inline-flex max-w-[220px] items-center gap-1 rounded border px-1.5 py-0.5 font-mono text-[10px] hover:border-acid hover:text-acid ${strong ? 'border-zinc-500 text-zinc-200' : 'border-line bg-panel text-zinc-400'}`}
        title={`${t?.name ?? ''}: открыть`}
      >
        <span style={{ color: t?.color }}>{t?.icon ?? '◇'}</span>
        <span className="truncate">{n.label}</span>
      </button>
    )
  }
  const nodeChip = (node || mentioned.length > 0) && (
    <div className="mb-1 flex flex-wrap gap-1">
      {node && chip(node, true)}
      {mentioned.map((n) => chip(n))}
    </div>
  )

  if (isSystem) {
    return (
      <div className="mx-auto my-1 max-w-[92%] rounded border border-dashed border-line px-3 py-1.5 text-center text-[11px] font-mono text-zinc-500">
        {message.content}
      </div>
    )
  }

  if (isUser) {
    return (
      <div className="flex flex-col items-end">
        {nodeChip}
        <div className="max-w-[90%] rounded-lg rounded-br-sm border border-zinc-600 bg-zinc-800/70 px-3 py-2 text-sm text-zinc-100">
          <div className="break-words">
            <Markdown text={message.content} nodes={nodes} types={types} onOpen={onNodeClick} variant="chat" />
          </div>
          <div className="mt-1 text-right text-[10px] font-mono text-zinc-500">ты · {time}</div>
        </div>
      </div>
    )
  }

  const to =
    message.addressee && message.addressee !== 'room'
      ? message.addressee === 'user'
        ? 'тебе'
        : agents.find((a) => a.id === message.addressee)?.name.split(' ')[0] ?? message.addressee
      : null

  return (
    <div className="flex gap-2">
      <div className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-sm border text-sm font-bold" style={{ borderColor: color, color }}>
        {AGENT_GLYPH[message.sender_id] ?? '?'}
      </div>
      <div className="min-w-0 max-w-[90%] flex-1">
        <div className="mb-0.5 flex flex-wrap items-baseline gap-x-2">
          <span className="text-xs font-semibold" style={{ color }}>
            {agent?.name.split(' (')[0] ?? message.sender_id}
          </span>
          {to && <span className="font-mono text-[10px] text-zinc-500">→ {to}</span>}
          {message.move && <span className="font-mono text-[9px] uppercase tracking-wider text-zinc-600">{message.move}</span>}
          <span className="text-[10px] font-mono text-zinc-600">{time}</span>
        </div>
        {node && <div className="mb-1 flex flex-wrap gap-1">{chip(node, true)}</div>}
        {showEgo && message.ego_thought && (
          <div className="mb-1 rounded border border-line bg-black/50 font-mono text-[11px]">
            <button onClick={() => setOpen((v) => !v)} className="flex w-full items-center gap-1 px-2 py-1 text-left text-zinc-500 hover:text-zinc-300">
              {open ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
              <Terminal size={11} />
              <span className="tracking-widest">мысль</span>
              {!open && <span className="ml-1 truncate normal-case tracking-normal text-zinc-600">{message.ego_thought}</span>}
            </button>
            {open && <div className="whitespace-pre-wrap break-words border-t border-line px-2.5 py-1.5 leading-relaxed text-zinc-400/90">{message.ego_thought}</div>}
          </div>
        )}
        <div
          className={`rounded-lg rounded-tl-sm border px-3 py-2 text-sm leading-relaxed ${silent ? 'border-dashed bg-transparent text-zinc-400' : 'bg-panel text-zinc-100'}`}
          style={{ borderColor: `${color}55` }}
        >
          <div className="break-words">
            <Markdown text={message.content} nodes={nodes} types={types} onOpen={onNodeClick} variant="chat" />
          </div>
        </div>
        {(message.actions ?? []).length > 0 && (
          <div className="mt-1 flex flex-wrap gap-1">
            {message.actions.map((a, i) => {
              const n = nodes.find((x) => x.id === a.object_id)
              const t = n ? types.find((x) => x.id === n.type_id) : undefined
              return (
                <button
                  key={i}
                  onClick={() => onNodeClick?.(a.object_id)}
                  className="inline-flex max-w-[280px] items-center gap-1 rounded border border-dashed px-1.5 py-0.5 font-mono text-[10px] text-zinc-300 hover:border-acid hover:text-acid"
                  style={{ borderColor: `${color}77` }}
                  title="Действие в библиотеке — открыть объект"
                >
                  <span style={{ color: t?.color ?? color }}>{a.op === 'link' ? '⇄' : a.op === 'update' ? '✎' : a.op === 'append' ? '✍' : a.op === 'note' ? '✎' : t?.icon ?? '＋'}</span>
                  <span className="truncate">{a.text}</span>
                </button>
              )
            })}
          </div>
        )}
        {mentioned.length > 0 && <div className="mt-1 flex flex-wrap gap-1">{mentioned.map((n) => chip(n))}</div>}
      </div>
    </div>
  )
}
