import { Eye, EyeOff, Skull } from 'lucide-react'
import type { Agent } from '../types'
import { AGENT_GLYPH } from '../types'
import { StatBar } from './StatBar'

interface Props {
  agent: Agent
  showAgenda: boolean
  onToggleAgenda: () => void
  speaking: boolean
  compact?: boolean
}

export function AgentCard({ agent, showAgenda, onToggleAgenda, speaking, compact }: Props) {
  const dead = agent.hp <= 0
  const color = dead ? '#55565f' : agent.color
  const shortName = agent.name.split(' ')[0]
  return (
    <div
      className={`relative flex min-w-0 flex-1 flex-col gap-1.5 rounded-md border bg-panel/80 px-2.5 py-2 transition-all ${
        speaking ? 'border-current' : 'border-line'
      } ${dead ? 'opacity-60 grayscale' : ''}`}
      style={{ color, borderColor: speaking ? color : undefined, boxShadow: speaking ? `0 0 18px ${color}55` : undefined }}
    >
      <div className="flex items-center gap-2" onClick={compact ? onToggleAgenda : undefined} role={compact ? 'button' : undefined}>
        <div
          className={`flex shrink-0 items-center justify-center rounded-sm border font-bold ${compact ? 'h-6 w-6 text-sm' : 'h-8 w-8 text-base'}`}
          style={{ borderColor: color, color }}
          title={agent.archetype}
        >
          {dead ? <Skull size={16} /> : AGENT_GLYPH[agent.id] ?? '?'}
        </div>
        <div className="min-w-0 flex-1">
          <div className="truncate text-xs font-semibold text-zinc-100" title={agent.name}>
            {compact ? shortName : agent.name}
          </div>
          {!compact && (
            <div className="truncate text-[10px] text-zinc-500" title={agent.archetype}>
              {dead ? 'HP = 0 · permadeath' : agent.archetype.split('.')[0]}
            </div>
          )}
        </div>
        {!compact && (
          <button
            onClick={onToggleAgenda}
            className="rounded p-1 text-zinc-500 hover:bg-zinc-800 hover:text-zinc-200"
            title="Скрытая цель (от LLM-Режиссёра)"
            aria-label="Показать скрытую цель"
          >
            {showAgenda ? <EyeOff size={14} /> : <Eye size={14} />}
          </button>
        )}
      </div>
      <StatBar label="HP" value={agent.hp} color={dead ? '#55565f' : '#FF0055'} />
      <StatBar label="DOP" value={agent.dopamine} color={dead ? '#55565f' : agent.color} />
      <div className="flex items-center justify-between text-[10px] font-mono text-zinc-500">
        <span>{compact ? 'уваж.' : 'уважение к тебе'}</span>
        <span className={agent.respect_user < 0 ? 'text-magenta' : agent.respect_user > 0 ? 'text-acid' : ''}>
          {agent.respect_user > 0 ? '+' : ''}
          {agent.respect_user}
        </span>
      </div>
      {showAgenda && (
        <div className="mt-1 rounded border border-dashed px-2 py-1.5 text-[11px] leading-snug text-zinc-300" style={{ borderColor: `${color}66` }}>
          <span className="mr-1 font-mono text-[9px] uppercase tracking-widest" style={{ color }}>
            hidden agenda
          </span>
          {agent.current_agenda || '—'}
        </div>
      )}
    </div>
  )
}
