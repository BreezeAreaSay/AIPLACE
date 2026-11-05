import { useState } from 'react'
import { BookOpen, ChevronDown, ChevronRight } from 'lucide-react'
import type { Agent, AgentMemory, DirectorLogEntry, Relation, RoomEventRecord } from '../types'
import { AGENT_COLORS } from '../types'
import { RelationsTriangle } from './RelationsTriangle'

interface Props {
  entry: DirectorLogEntry | null
  agents: Agent[]
  directorModel: string
  relations: Relation[]
  memories: AgentMemory[]
  events: RoomEventRecord[]
}

function eventLine(e: RoomEventRecord, agents: Agent[]): string {
  let p: Record<string, unknown> = {}
  try {
    p = JSON.parse(e.payload)
  } catch {
    /* ignore */
  }
  const who = (id: unknown) => agents.find((a) => a.id === id)?.name.split(' ')[0] ?? String(id ?? '—')
  switch (e.kind) {
    case 'room_opened':
      return 'комната открыта'
    case 'contract_created':
      return `🤝 слово #${p.id} «${p.title}» · поручитель ${who(p.guarantor)} · ${p.stake} HP`
    case 'contract_done':
      return `✅ слово #${p.id} сдержано · ${who(p.guarantor)} +HP`
    case 'contract_failed':
      return `💀 слово #${p.id} нарушено (${p.by}) · ${who(p.guarantor)} −${p.stake} HP`
    case 'contract_cancelled':
      return `✖ слово #${p.id} отозвано`
    case 'agent_died':
      return `☠ ${who(p.agent)} умер (${p.cause})`
    default:
      return `${e.kind} ${e.payload}`
  }
}

function Diaries({ memories, agents }: { memories: AgentMemory[]; agents: Agent[] }) {
  const [open, setOpen] = useState<Record<string, boolean>>({})
  if (memories.length === 0) return <div className="text-[11px] text-zinc-600">Дневники пусты — заполняются после первых ~8 сообщений.</div>
  return (
    <div className="space-y-1.5">
      {memories.map((m) => {
        const a = agents.find((x) => x.id === m.agent_id)
        const isOpen = !!open[m.agent_id]
        return (
          <div key={m.agent_id} className="rounded border border-line bg-panel">
            <button onClick={() => setOpen((o) => ({ ...o, [m.agent_id]: !isOpen }))} className="flex w-full items-center gap-1.5 px-2.5 py-1.5 text-left">
              {isOpen ? <ChevronDown size={12} className="text-zinc-500" /> : <ChevronRight size={12} className="text-zinc-500" />}
              <BookOpen size={12} style={{ color: AGENT_COLORS[m.agent_id] }} />
              <span className="text-xs font-medium" style={{ color: AGENT_COLORS[m.agent_id] }}>
                {a?.name ?? m.agent_id}
              </span>
              <span className="ml-auto font-mono text-[10px] text-zinc-600">до #{m.covered_until}</span>
            </button>
            {isOpen && <div className="whitespace-pre-wrap border-t border-line px-3 py-2 text-[12px] leading-relaxed text-zinc-300">{m.summary}</div>}
          </div>
        )
      })}
    </div>
  )
}

function name(id: string, agents: Agent[]) {
  return agents.find((a) => a.id === id)?.name.split(' (')[0] ?? id
}

function fmt(n: number) {
  return n > 0 ? `+${n}` : `${n}`
}

export function DirectorInspector({ entry, agents, directorModel, relations, memories, events }: Props) {
  const scene = (
    <>
      <section className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <div>
          <h4 className="mb-1.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">отношения</h4>
          <RelationsTriangle agents={agents} relations={relations} />
          <div className="space-y-0.5 font-mono text-[10px] text-zinc-500">
            {relations.map((r) => (
              <div key={r.a_id + r.b_id} className="truncate" title={r.label}>
                <span style={{ color: AGENT_COLORS[r.a_id] }}>{name(r.a_id, agents)}</span>
                {' ↔ '}
                <span style={{ color: AGENT_COLORS[r.b_id] }}>{name(r.b_id, agents)}</span>
                <span className={r.affinity < 0 ? ' text-magenta' : r.affinity > 0 ? ' text-acid' : ''}> {fmt(r.affinity)}</span>
                {r.label && <span className="text-zinc-600"> · {r.label}</span>}
              </div>
            ))}
          </div>
        </div>
        <div>
          <h4 className="mb-1.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">дневники · долговременная память</h4>
          <Diaries memories={memories} agents={agents} />
          <h4 className="mb-1.5 mt-3 font-mono text-[10px] uppercase tracking-widest text-zinc-500">лента событий</h4>
          <div className="max-h-40 space-y-0.5 overflow-y-auto font-mono text-[10px] text-zinc-400">
            {events.length === 0 && <div className="text-zinc-600">пусто</div>}
            {[...events].reverse().map((e) => (
              <div key={e.id} className={e.narrated ? '' : 'text-zinc-200'}>
                <span className="text-zinc-600">{new Date(e.created_at).toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' })}</span> {eventLine(e, agents)}
              </div>
            ))}
          </div>
        </div>
      </section>
    </>
  )
  if (!entry) {
    return (
      <div className="space-y-4 p-3 text-xs">
        {scene}
        <div className="font-mono text-zinc-500">Режиссёр ещё не выходил на сцену. Отправь сообщение или нажми «Автономный тик».</div>
      </div>
    )
  }
  const o = entry.output
  const time = new Date(entry.created_at).toLocaleTimeString('ru-RU')
  return (
    <div className="space-y-4 p-3 text-xs">
      {scene}
      <div className="border-t border-line" />
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
        <h4 className="mb-1.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">сцена · биты</h4>
        <div className="space-y-1.5">
          {(o.scene ?? []).map((b, i) => (
            <div key={i} className="rounded border-l-2 bg-panel px-2.5 py-1.5" style={{ borderColor: AGENT_COLORS[b.speaker] ?? '#666' }}>
              <div className="flex flex-wrap items-baseline gap-x-2 font-mono text-[10px]">
                <span style={{ color: AGENT_COLORS[b.speaker] }}>{name(b.speaker, agents)}</span>
                <span className="text-zinc-500">→ {b.addressee === 'user' ? 'игроку' : b.addressee === 'room' ? 'комнате' : name(b.addressee ?? '', agents)}</span>
                {b.move && <span className="uppercase tracking-wider text-zinc-400">{b.move}</span>}
                {b.delivery && <span className="text-zinc-600">· {b.delivery}</span>}
              </div>
              {b.intent && <div className="mt-0.5 text-zinc-300">{b.intent}</div>}
            </div>
          ))}
          {(o.scene?.length ?? 0) === 0 && o.next_speakers.map((id) => <div key={id} style={{ color: AGENT_COLORS[id] }}>{name(id, agents)}</div>)}
        </div>
      </section>

      {Object.keys(o.inner_states ?? {}).length > 0 && (
        <section>
          <h4 className="mb-1.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">inner_states · состояния</h4>
          <div className="space-y-1">
            {Object.entries(o.inner_states).map(([id, st]) => (
              <div key={id} className="text-[11px] text-zinc-300">
                <span className="font-mono text-[10px]" style={{ color: AGENT_COLORS[id] }}>{name(id, agents)}</span> — {st}
              </div>
            ))}
          </div>
        </section>
      )}

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

      {((o.new_contracts?.length ?? 0) > 0 || (o.contract_updates?.length ?? 0) > 0) && (
        <section>
          <h4 className="mb-1.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">contracts · слово игрока</h4>
          <div className="space-y-1 text-[11px]">
            {(o.new_contracts ?? []).map((c, i) => (
              <div key={`c${i}`} className="text-zinc-300">
                🤝 «{c.title}» · поручитель <span style={{ color: AGENT_COLORS[c.guarantor] }}>{name(c.guarantor, agents)}</span> · ставка {c.stake ?? 15}
                {c.deadline && <span className="text-zinc-500"> · до {new Date(c.deadline).toLocaleString('ru-RU', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' })}</span>}
                {c.why && <span className="text-zinc-500"> — {c.why}</span>}
              </div>
            ))}
            {(o.contract_updates ?? []).map((u, i) => (
              <div key={`u${i}`} className={u.status === 'done' ? 'text-acid' : 'text-magenta'}>
                вердикт по #{String(u.id)}: {u.status} {u.note && <span className="text-zinc-500">— {u.note}</span>}
              </div>
            ))}
          </div>
        </section>
      )}

      {(o.relation_deltas?.length ?? 0) > 0 && (
        <section>
          <h4 className="mb-1.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">relation_deltas</h4>
          <div className="space-y-0.5 font-mono text-[11px]">
            {(o.relation_deltas ?? []).map((d, i) => (
              <div key={i} className="text-zinc-300">
                <span style={{ color: AGENT_COLORS[d.a] }}>{name(d.a, agents)}</span> ↔ <span style={{ color: AGENT_COLORS[d.b] }}>{name(d.b, agents)}</span>
                <span className={d.delta < 0 ? ' text-magenta' : ' text-acid'}> {fmt(d.delta)}</span>
                {d.label && <span className="text-zinc-500"> · {d.label}</span>}
              </div>
            ))}
          </div>
        </section>
      )}

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
