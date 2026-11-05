import { X } from 'lucide-react'
import type { Agent, AgentMemory, Message, Relation } from '../types'
import { glyphOf } from '../types'
import { StatBar } from './StatBar'
import { RelationsTriangle } from './RelationsTriangle'

interface Props {
  agent: Agent
  agents: Agent[]
  memory: AgentMemory | undefined
  relations: Relation[]
  messages: Message[]
  onClose: () => void
}

export function AgentProfile({ agent, agents, memory, relations, messages, onClose }: Props) {
  const dead = agent.hp <= 0
  const color = dead ? '#55565f' : agent.color
  const thoughts = messages.filter((m) => m.sender_id === agent.id && m.ego_thought).slice(-4).reverse()
  return (
    <div className="fixed inset-0 z-50 flex items-end justify-center bg-black/70 sm:items-center sm:p-4" onClick={onClose}>
      <div className="max-h-[92vh] w-full max-w-2xl overflow-y-auto rounded-t-xl border border-line bg-void sm:rounded-xl" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-start gap-3 border-b border-line px-4 py-3">
          <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded border text-2xl font-bold" style={{ borderColor: color, color }}>
            {dead ? '☠' : glyphOf(agent)}
          </div>
          <div className="min-w-0 flex-1">
            <div className="text-base font-semibold text-zinc-100">{agent.name}</div>
            <div className="text-xs text-zinc-500">{agent.archetype}</div>
            {agent.inner_state && <div className="mt-1 text-sm italic text-zinc-300">«{agent.inner_state}»</div>}
          </div>
          <button onClick={onClose} className="rounded p-1 text-zinc-500 hover:text-zinc-200" aria-label="Закрыть">
            <X size={16} />
          </button>
        </div>
        <div className="grid gap-4 p-4 sm:grid-cols-2">
          <div className="space-y-4">
            <section className="space-y-1.5">
              <h4 className="font-mono text-[10px] uppercase tracking-widest text-zinc-500">состояние</h4>
              <StatBar label="HP" value={agent.hp} color="#FF0055" />
              <StatBar label="DOP" value={agent.dopamine} color={agent.color} />
              <div className="flex justify-between font-mono text-[10px] text-zinc-500">
                <span>уважение к тебе</span>
                <span className={agent.respect_user < 0 ? 'text-magenta' : agent.respect_user > 0 ? 'text-acid' : ''}>{agent.respect_user > 0 ? '+' : ''}{agent.respect_user}</span>
              </div>
            </section>
            <section>
              <h4 className="mb-1 font-mono text-[10px] uppercase tracking-widest text-zinc-500">скрытая цель сейчас</h4>
              <div className="rounded border border-dashed px-2.5 py-1.5 text-xs text-zinc-300" style={{ borderColor: `${color}66` }}>
                {agent.current_agenda || '—'}
              </div>
            </section>
            <section>
              <h4 className="mb-1 font-mono text-[10px] uppercase tracking-widest text-zinc-500">отношения</h4>
              <RelationsTriangle agents={agents} relations={relations} size={190} />
            </section>
          </div>
          <div className="space-y-4">
            <section>
              <h4 className="mb-1 font-mono text-[10px] uppercase tracking-widest text-zinc-500">дневник · память</h4>
              {memory ? (
                <div className="whitespace-pre-wrap rounded border border-line bg-panel px-3 py-2 text-[12px] leading-relaxed text-zinc-300">{memory.summary}</div>
              ) : (
                <div className="text-[11px] text-zinc-600">Дневник пуст — заполнится после первых ~8 реплик.</div>
              )}
            </section>
            <section>
              <h4 className="mb-1 font-mono text-[10px] uppercase tracking-widest text-zinc-500">последние мысли</h4>
              <div className="space-y-1">
                {thoughts.length === 0 && <div className="text-[11px] text-zinc-600">ещё не думал вслух</div>}
                {thoughts.map((m) => (
                  <div key={m.id} className="rounded border border-line bg-black/40 px-2.5 py-1.5 font-mono text-[11px] leading-snug text-zinc-400">
                    {m.ego_thought}
                  </div>
                ))}
              </div>
            </section>
          </div>
        </div>
      </div>
    </div>
  )
}
