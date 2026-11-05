import { useState } from 'react'
import { Check, Handshake, Hourglass, Skull, X } from 'lucide-react'
import type { Agent, Contract } from '../types'
import { AGENT_COLORS } from '../types'

interface Props {
  contracts: Contract[]
  agents: Agent[]
  now: string
  disabled: boolean
  onResolve: (id: number, status: 'done' | 'failed' | 'cancelled') => void
  onPromise: (text: string) => void
}

function remaining(deadline: string | null, now: string): { text: string; urgent: boolean; overdue: boolean } {
  if (!deadline) return { text: 'без срока', urgent: false, overdue: false }
  const ms = new Date(deadline).getTime() - new Date(now || Date.now()).getTime()
  if (ms <= 0) return { text: 'просрочено', urgent: true, overdue: true }
  const h = Math.floor(ms / 3.6e6)
  const d = Math.floor(h / 24)
  const m = Math.floor((ms % 3.6e6) / 6e4)
  const text = d > 0 ? `${d} д ${h % 24} ч` : h > 0 ? `${h} ч ${m} мин` : `${m} мин`
  return { text: `осталось ${text}`, urgent: ms < 6 * 3.6e6, overdue: false }
}

export function ContractsPanel({ contracts, agents, now, disabled, onResolve, onPromise }: Props) {
  const [title, setTitle] = useState('')
  const [when, setWhen] = useState('')
  const open = contracts.filter((c) => c.status === 'open')
  const closed = contracts.filter((c) => c.status !== 'open')
  const name = (id: string | null) => agents.find((a) => a.id === id)?.name.split(' ')[0] ?? '—'

  const submit = () => {
    const t = title.trim()
    if (!t) return
    const whenText = when ? ` до ${new Date(when).toLocaleString('ru-RU', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' })} (${new Date(when).toISOString()})` : ''
    onPromise(`Даю слово: ${t}${whenText}. Кто из вас поручится?`)
    setTitle('')
    setWhen('')
  }

  return (
    <div className="space-y-4 p-3 text-sm">
      <div className="rounded border border-line bg-panel p-3">
        <div className="mb-2 flex items-center gap-2 font-mono text-[10px] uppercase tracking-widest text-zinc-500">
          <Handshake size={12} className="text-acid" /> дать слово
        </div>
        <div className="flex flex-col gap-2 sm:flex-row">
          <input
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="напишу документацию к API"
            className="flex-1 rounded border border-line bg-void px-3 py-1.5 text-xs text-zinc-100 placeholder:text-zinc-600 focus:border-acid focus:outline-none"
          />
          <input
            type="datetime-local"
            value={when}
            onChange={(e) => setWhen(e.target.value)}
            className="rounded border border-line bg-void px-2 py-1.5 font-mono text-xs text-zinc-300 focus:border-acid focus:outline-none"
          />
          <button
            onClick={submit}
            disabled={disabled || !title.trim()}
            className="rounded bg-acid px-3 py-1.5 font-mono text-xs font-bold text-void disabled:opacity-40"
          >
            в комнату
          </button>
        </div>
        <div className="mt-1.5 text-[11px] text-zinc-500">
          Обещание уйдёт репликой — Режиссёр оформит контракт и назначит поручителя, который поставит на тебя свои HP. Можно и просто написать в чат «обещаю…».
        </div>
      </div>

      <section>
        <h4 className="mb-2 font-mono text-[10px] uppercase tracking-widest text-zinc-500">открытые · {open.length}</h4>
        {open.length === 0 && <div className="text-xs text-zinc-600">Пока ни одного слова. Агенты это заметят.</div>}
        <div className="space-y-2">
          {open.map((c) => {
            const r = remaining(c.deadline, now)
            const color = c.guarantor_id ? AGENT_COLORS[c.guarantor_id] : '#888'
            return (
              <div key={c.id} className="rounded border-l-2 bg-panel px-3 py-2" style={{ borderColor: color }}>
                <div className="flex items-start gap-2">
                  <div className="min-w-0 flex-1">
                    <div className="text-zinc-100">
                      <span className="mr-1.5 font-mono text-[10px] text-zinc-500">#{c.id}</span>
                      {c.title}
                    </div>
                    {c.details && <div className="mt-0.5 text-[11px] text-zinc-400">{c.details}</div>}
                    <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 font-mono text-[10px]">
                      <span className={r.overdue ? 'text-magenta' : r.urgent ? 'text-amber-400' : 'text-zinc-400'}>
                        <Hourglass size={10} className="mr-1 inline" />
                        {r.text}
                        {c.deadline && <span className="text-zinc-600"> · {new Date(c.deadline).toLocaleString('ru-RU', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' })}</span>}
                      </span>
                      <span style={{ color }}>поручитель {name(c.guarantor_id)}</span>
                      <span className="text-zinc-500">ставка {c.stake} HP</span>
                    </div>
                  </div>
                  <div className="flex shrink-0 gap-1">
                    <button
                      onClick={() => onResolve(c.id, 'done')}
                      disabled={disabled}
                      className="rounded border border-acid/50 p-1.5 text-acid hover:bg-acid/10 disabled:opacity-40"
                      title="Сделал"
                    >
                      <Check size={14} />
                    </button>
                    <button
                      onClick={() => {
                        if (confirm(`Признать слово #${c.id} нарушенным? Поручитель потеряет ${c.stake} HP.`)) onResolve(c.id, 'failed')
                      }}
                      disabled={disabled}
                      className="rounded border border-magenta/50 p-1.5 text-magenta hover:bg-magenta/10 disabled:opacity-40"
                      title="Провалил"
                    >
                      <Skull size={14} />
                    </button>
                    <button
                      onClick={() => onResolve(c.id, 'cancelled')}
                      disabled={disabled}
                      className="rounded border border-line p-1.5 text-zinc-500 hover:text-zinc-200 disabled:opacity-40"
                      title="Отозвать (без последствий)"
                    >
                      <X size={14} />
                    </button>
                  </div>
                </div>
              </div>
            )
          })}
        </div>
      </section>

      {closed.length > 0 && (
        <section>
          <h4 className="mb-2 font-mono text-[10px] uppercase tracking-widest text-zinc-500">история</h4>
          <div className="space-y-1">
            {closed.map((c) => (
              <div key={c.id} className="flex items-center gap-2 text-[11px] text-zinc-400">
                <span className={c.status === 'done' ? 'text-acid' : c.status === 'failed' ? 'text-magenta' : 'text-zinc-600'}>
                  {c.status === 'done' ? '✅' : c.status === 'failed' ? '💀' : '✖'}
                </span>
                <span className="font-mono text-[10px] text-zinc-600">#{c.id}</span>
                <span className="truncate">{c.title}</span>
                <span className="ml-auto shrink-0 font-mono text-[10px]" style={{ color: c.guarantor_id ? AGENT_COLORS[c.guarantor_id] : undefined }}>
                  {name(c.guarantor_id)}
                </span>
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  )
}
