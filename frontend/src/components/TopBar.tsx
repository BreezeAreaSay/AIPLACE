import { useState } from 'react'
import { Radio, RotateCcw, Zap } from 'lucide-react'
import type { Agent } from '../types'
import { AgentCard } from './AgentCard'

interface Props {
  agents: Agent[]
  speaking: string | null
  live: boolean
  mockMode: boolean
  model: string
  onReset: () => void
  compact: boolean
}

export function TopBar({ agents, speaking, live, mockMode, model, onReset, compact }: Props) {
  const [showAll, setShowAll] = useState(false)
  const [open, setOpen] = useState<Record<string, boolean>>({})
  const toggle = (id: string) => setOpen((o) => ({ ...o, [id]: !o[id] }))
  return (
    <header className="relative z-20 border-b border-line bg-void/95 backdrop-blur" style={{ paddingTop: 'var(--tg-safe-top)' }}>
      <div className="flex items-center gap-2 px-3 py-2">
        <Zap size={16} className="text-acid" />
        <span className="font-mono text-sm font-bold tracking-[0.2em] text-zinc-100">TRIAD CANVAS</span>
        <span className="hidden text-[10px] font-mono text-zinc-600 sm:inline">киберпсихологическая комната</span>
        <div className="ml-auto flex items-center gap-2 text-[10px] font-mono">
          {mockMode ? (
            <span className="rounded border border-magenta/50 px-1.5 py-0.5 text-magenta" title="Нет OPENROUTER_API_KEY — ответы шаблонные">
              MOCK LLM
            </span>
          ) : (
            <span className="hidden rounded border border-line px-1.5 py-0.5 text-zinc-500 sm:inline" title="OPENROUTER_MODEL">
              {model}
            </span>
          )}
          <span className={`flex items-center gap-1 ${live ? 'text-acid' : 'text-zinc-600'}`} title="SSE live stream">
            <Radio size={12} className={live ? 'animate-pulseLine' : ''} />
            {live ? 'live' : 'offline'}
          </span>
          <button
            onClick={() => setShowAll((v) => !v)}
            className="rounded border border-line px-1.5 py-0.5 text-zinc-400 hover:border-acid hover:text-acid"
            title="Показать все скрытые цели"
          >
            {showAll ? 'скрыть цели' : 'цели'}
          </button>
          <button
            onClick={() => {
              if (confirm('Стереть комнату и заселить Триаду заново?')) onReset()
            }}
            className="rounded border border-line p-1 text-zinc-500 hover:border-magenta hover:text-magenta"
            title="Новая комната (сброс состояния)"
          >
            <RotateCcw size={12} />
          </button>
        </div>
      </div>
      <div className="flex gap-2 overflow-x-auto px-3 pb-2">
        {agents.map((a) => (
          <AgentCard
            key={a.id}
            agent={a}
            compact={compact}
            speaking={speaking === a.id}
            showAgenda={showAll || !!open[a.id]}
            onToggleAgenda={() => toggle(a.id)}
          />
        ))}
      </div>
    </header>
  )
}
