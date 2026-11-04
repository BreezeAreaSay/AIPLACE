import type { Agent, DirectorLogEntry } from '../types'
import { AGENT_COLORS } from '../types'

interface Props {
  entry: DirectorLogEntry | null
  agents: Agent[]
  directorModel: string
}

function name(id: string, agents: Agent[]) {
  return agents.find((a) => a.id === id)?.name.split(' (')[0] ?? id
}

function fmt(n: number) {
  return n > 0 ? `+${n}` : `${n}`
}

export function DirectorInspector({ entry, agents, directorModel }: Props) {
  if (!entry) {
    return (
      <div className="p-4 text-xs font-mono text-zinc-500">
        Режиссёр ещё не выходил на сцену. Отправь сообщение или нажми «Автономный тик».
      </div>
    )
  }
  const o = entry.output
  const time = new Date(entry.created_at).toLocaleTimeString('ru-RU')
  return (
    <div className="space-y-4 p-3 text-xs">
      <div className="flex flex-wrap items-center gap-2 font-mono text-[10px]">
        <span className="rounded border border-line px-1.5 py-0.5 text-zinc-400">#{entry.id}</span>
        <span className="rounded border border-line px-1.5 py-0.5 text-zinc-400">
          {entry.trigger === 'tick' ? '🎲 автономный тик' : '💬 реплика игрока'}
        </span>
        <span className={`rounded border px-1.5 py-0.5 ${entry.source === 'llm' ? 'border-acid/40 text-acid' : 'border-magenta/40 text-magenta'}`}>
          {entry.source === 'llm' ? `LLM · ${directorModel}` : 'fallback (модель не ответила)'}
        </span>
        <span className="text-zinc-600">{time}</span>
      </div>

      {(o.room_mood || o.director_note) && (
        <div className="rounded border border-line bg-panel p-2.5">
          {o.room_mood && (
            <div className="mb-1 font-mono text-[10px] uppercase tracking-widest text-zinc-500">
              атмосфера: <span className="text-zinc-200">{o.room_mood}</span>
            </div>
          )}
          {o.director_note && <div className="italic text-zinc-300">«{o.director_note}»</div>}
        </div>
      )}

      <section>
        <h4 className="mb-1.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">next_speakers</h4>
        <div className="flex flex-wrap gap-1.5">
          {o.next_speakers.map((id, i) => (
            <span key={id} className="rounded border px-2 py-0.5 font-medium" style={{ borderColor: AGENT_COLORS[id], color: AGENT_COLORS[id] }}>
              {i + 1}. {name(id, agents)}
            </span>
          ))}
        </div>
      </section>

      <section>
        <h4 className="mb-1.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">updated_agendas · скрытые цели</h4>
        <div className="space-y-1.5">
          {Object.entries(o.updated_agendas).map(([id, goal]) => (
            <div key={id} className="rounded border-l-2 bg-panel px-2.5 py-1.5" style={{ borderColor: AGENT_COLORS[id] ?? '#666' }}>
              <div className="mb-0.5 font-mono text-[10px]" style={{ color: AGENT_COLORS[id] }}>
                {name(id, agents)}
              </div>
              <div className="text-zinc-200">{goal}</div>
            </div>
          ))}
          {Object.keys(o.updated_agendas).length === 0 && <div className="text-zinc-600">повестки не менялись</div>}
        </div>
      </section>

      <section>
        <h4 className="mb-1.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">stat_deltas · кормление</h4>
        <table className="w-full font-mono text-[11px]">
          <thead className="text-zinc-600">
            <tr>
              <th className="text-left font-normal">агент</th>
              <th className="font-normal">hp</th>
              <th className="font-normal">dop</th>
              <th className="font-normal">resp</th>
              <th className="text-left font-normal">почему</th>
            </tr>
          </thead>
          <tbody>
            {Object.entries(o.stat_deltas).map(([id, d]) => (
              <tr key={id} className="border-t border-line/60">
                <td className="py-1" style={{ color: AGENT_COLORS[id] }}>
                  {name(id, agents)}
                </td>
                <td className={`text-center ${d.hp < 0 ? 'text-magenta' : d.hp > 0 ? 'text-acid' : 'text-zinc-500'}`}>{fmt(d.hp)}</td>
                <td className={`text-center ${d.dopamine < 0 ? 'text-magenta' : d.dopamine > 0 ? 'text-acid' : 'text-zinc-500'}`}>{fmt(d.dopamine)}</td>
                <td className={`text-center ${d.respect_user < 0 ? 'text-magenta' : d.respect_user > 0 ? 'text-acid' : 'text-zinc-500'}`}>{fmt(d.respect_user)}</td>
                <td className="py-1 pl-2 text-zinc-400">{d.reason ?? ''}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <section>
        <h4 className="mb-1.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">graph_mutations</h4>
        <div className="space-y-1 font-mono text-[11px]">
          {o.graph_mutations.nodes.map((n, i) => (
            <div key={`n${i}`} className="text-zinc-300">
              <span style={{ color: AGENT_COLORS[n.author] ?? '#aaa' }}>●</span> +node <span className="text-zinc-500">[{n.type}]</span> {n.label}
            </div>
          ))}
          {o.graph_mutations.edges.map((e, i) => (
            <div key={`e${i}`} className="text-zinc-400">
              ↳ {e.from} <span className="text-zinc-600">—{e.label}→</span> {e.to}
            </div>
          ))}
          {o.graph_mutations.nodes.length + o.graph_mutations.edges.length === 0 && <div className="text-zinc-600">граф не менялся</div>}
        </div>
      </section>
    </div>
  )
}
