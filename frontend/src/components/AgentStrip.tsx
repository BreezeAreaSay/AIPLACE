import type { Agent } from '../types'
import { AGENT_GLYPH } from '../types'

interface Props {
  agents: Agent[]
  speaking: string | null
  onOpen: (id: string) => void
}

/** Three faces of the room: glyph, name, one line of state. Numbers live in the profile. */
export function AgentStrip({ agents, speaking, onOpen }: Props) {
  return (
    <div className="grid grid-cols-3 gap-1.5 border-b border-line px-2 py-2">
      {agents.map((a) => {
        const dead = a.hp <= 0
        const color = dead ? '#55565f' : a.color
        const active = speaking === a.id
        return (
          <button
            key={a.id}
            onClick={() => onOpen(a.id)}
            className={`flex min-w-0 flex-col items-start gap-1 rounded-md border bg-panel/80 px-2 py-1.5 text-left transition ${dead ? 'opacity-60 grayscale' : ''}`}
            style={{ borderColor: active ? color : '#23242e', boxShadow: active ? `0 0 18px ${color}55` : undefined }}
            title={a.archetype}
          >
            <div className="flex w-full items-center gap-1.5">
              <span className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-sm border text-sm font-bold ${active ? 'animate-pulseLine' : ''}`} style={{ borderColor: color, color }}>
                {dead ? '☠' : AGENT_GLYPH[a.id]}
              </span>
              <span className="truncate text-xs font-semibold text-zinc-100">{a.name.split(' ')[0]}</span>
              <span className="ml-auto h-1.5 w-1.5 shrink-0 rounded-full" style={{ background: dead ? '#55565f' : a.hp < 30 ? '#FF0055' : a.dopamine < 25 ? '#f59e0b' : color }} title={`HP ${a.hp} · дофамин ${a.dopamine}`} />
            </div>
            <div className="line-clamp-2 w-full text-[10px] leading-snug text-zinc-400">
              {dead ? 'больше не отвечает' : a.inner_state || a.archetype.split('.')[0]}
            </div>
          </button>
        )
      })}
    </div>
  )
}
