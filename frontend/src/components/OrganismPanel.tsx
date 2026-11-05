import { useEffect, useState } from 'react'
import { Activity, BookOpen, Leaf, RefreshCw, ShieldCheck, Sparkles, Zap } from 'lucide-react'
import type { Agent, GraphNode, ObjectType, Proposal } from '../types'
import { ProposalCard } from './ProposalCard'
import { AGENT_COLORS } from '../types'
import { api, type OrganismState } from '../api'

interface Props {
  agents: Agent[]
  nodes: GraphNode[]
  types: ObjectType[]
  disabled: boolean
  onSelect: (id: string) => void
  onTend: (nodeId?: string) => Promise<unknown>
  onChronicle: () => Promise<unknown>
  onAudit: () => Promise<unknown>
  proposals: Proposal[]
  onAcceptProposal: (id: number) => Promise<unknown>
  onRejectProposal: (id: number) => Promise<unknown>
}

function Tile({ label, value, tone, hint }: { label: string; value: number | string; tone?: 'good' | 'warn' | 'bad'; hint?: string }) {
  const color = tone === 'good' ? 'text-acid' : tone === 'warn' ? 'text-[#fbbf24]' : tone === 'bad' ? 'text-magenta' : 'text-zinc-200'
  return (
    <div className="rounded border border-line bg-panel/60 px-2 py-1.5" title={hint}>
      <div className={`text-base font-semibold ${color}`}>{value}</div>
      <div className="font-mono text-[9px] uppercase tracking-wider text-zinc-500">{label}</div>
    </div>
  )
}

/** Пульс библиотеки: как о ней заботятся, что требует внимания, что жители сделали сами. */
export function OrganismPanel({ agents, nodes, types, disabled, onSelect, onTend, onChronicle, onAudit, proposals, onAcceptProposal, onRejectProposal }: Props) {
  const [st, setSt] = useState<OrganismState | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [msg, setMsg] = useState<string | null>(null)
  const load = () => void api.organism().then(setSt).catch(() => undefined)
  useEffect(() => {
    load()
    const t = setInterval(load, 30_000)
    return () => clearInterval(t)
  }, [nodes.length])
  const rules = nodes.filter((n) => n.type_id === 'rule' && n.archived === 0)
  const name = (id: string) => agents.find((a) => a.id === id)?.name.split(' ')[0] ?? id
  const node = (id: string) => nodes.find((n) => n.id === id)
  const icon = (id: string) => {
    const n = node(id)
    const t = n ? types.find((x) => x.id === n.type_id) : undefined
    return t?.icon ?? '◇'
  }
  const run = async (key: string, fn: () => Promise<unknown>, done: string) => {
    setBusy(key)
    setMsg(null)
    try {
      await fn()
      setMsg(done)
      load()
    } catch (e) {
      setMsg(`ошибка: ${(e as Error).message}`)
    } finally {
      setBusy(null)
      window.setTimeout(() => setMsg(null), 4000)
    }
  }
  if (!st) return <div className="p-4 font-mono text-xs text-zinc-600">слушаю пульс…</div>
  const h = st.health
  const tone = h.score >= 75 ? 'good' : h.score >= 45 ? 'warn' : 'bad'

  return (
    <div className="space-y-4 p-3">
      <section>
        <div className="mb-1.5 flex items-center gap-2 font-mono text-[10px] uppercase tracking-widest text-zinc-500">
          <Activity size={11} /> пульс библиотеки
          <button onClick={load} className="ml-auto text-zinc-600 hover:text-acid" title="Обновить">
            <RefreshCw size={11} />
          </button>
        </div>
        <div className="mb-2 flex items-end gap-3">
          <div className={`text-4xl font-bold ${tone === 'good' ? 'text-acid' : tone === 'warn' ? 'text-[#fbbf24]' : 'text-magenta'}`}>{h.score}</div>
          <div className="pb-1 text-[11px] text-zinc-400">
            {h.score >= 75 ? 'ухоженная: связи есть, файлы прочитаны, задачи дышат' : h.score >= 45 ? 'живёт, но есть сироты и залежи' : 'запущена: много несвязанного и непрочитанного'}
          </div>
        </div>
        <div className="grid grid-cols-3 gap-1.5 sm:grid-cols-4">
          <Tile label="объектов" value={h.objects} />
          <Tile label="выросло за 7 дн" value={`+${h.grown_7d}`} tone="good" />
          <Tile label="тронули жители" value={h.touched_by_agents_7d} hint="объектов, которые за неделю меняли персонажи" />
          <Tile label="сирот" value={h.orphans} tone={h.orphans ? 'warn' : 'good'} hint="без связей и упоминаний" />
          <Tile label="залежалось" value={h.stale} tone={h.stale ? 'warn' : 'good'} hint="проекты, цели, идеи без движения 30+ дней" />
          <Tile label="файлов не прочитано" value={h.undigested_files} tone={h.undigested_files ? 'bad' : 'good'} />
          <Tile label="задач открыто" value={h.open_tasks} hint={`просрочено: ${h.overdue_tasks}`} tone={h.overdue_tasks ? 'warn' : undefined} />
          <Tile label="закладок не читано" value={h.unread_bookmarks} />
        </div>
      </section>

      <section>
        <div className="mb-1.5 flex items-center gap-2 font-mono text-[10px] uppercase tracking-widest text-zinc-500">
          <ShieldCheck size={11} /> предложения жителей · {proposals.length}
          <button onClick={() => void run('audit', onAudit, 'аудит проведён')} disabled={disabled || !!busy} className="ml-auto rounded border border-line px-1.5 py-0.5 text-[10px] normal-case tracking-normal text-zinc-300 hover:border-acid hover:text-acid disabled:opacity-40">
            {busy === 'audit' ? 'смотрят…' : 'аудит: дубли, пустышки, теги'}
          </button>
        </div>
        {proposals.length === 0 && <div className="text-[11px] text-zinc-600">Пока предложений нет. Жители предлагают слить дубли, убрать пустышки, завести сущности и типы — ты подтверждаешь.</div>}
        <div className="space-y-1">
          {proposals.map((p) => (
            <ProposalCard key={p.id} proposal={p} agents={agents} nodes={nodes} types={types} onAccept={onAcceptProposal} onReject={onRejectProposal} onOpen={onSelect} />
          ))}
        </div>
      </section>

      <section>
        <div className="mb-1.5 flex items-center gap-2 font-mono text-[10px] uppercase tracking-widest text-zinc-500">
          <Leaf size={11} /> требует заботы · {st.needs_care.length}
        </div>
        {st.needs_care.length === 0 && <div className="text-[11px] text-zinc-600">Всё прибрано. Принеси файл или заведи что-нибудь — двор разберёт.</div>}
        <div className="space-y-1">
          {st.needs_care.map((c) => (
            <div key={c.node_id} className="flex items-center gap-2 rounded border border-line/60 bg-panel/40 px-2 py-1 text-[11px]">
              <span>{icon(c.node_id)}</span>
              <button onClick={() => onSelect(c.node_id)} className="min-w-0 flex-1 truncate text-left text-zinc-300 hover:underline">
                {c.reason}
              </button>
              <span className="shrink-0 font-mono text-[9px] text-zinc-600">{c.kind}</span>
              <button
                onClick={() => void run(c.node_id, () => onTend(c.node_id), 'разобрано')}
                disabled={disabled || !!busy}
                className="shrink-0 rounded border border-line px-1.5 py-0.5 font-mono text-[10px] text-zinc-300 hover:border-acid hover:text-acid disabled:opacity-40"
              >
                {busy === c.node_id ? '…' : 'разобрать'}
              </button>
            </div>
          ))}
        </div>
        <div className="mt-2 flex flex-wrap gap-1.5">
          <button onClick={() => void run('next', () => onTend(), 'готово')} disabled={disabled || !!busy || st.needs_care.length === 0} className="flex items-center gap-1 rounded border border-acid/50 px-2 py-1 font-mono text-[11px] text-acid hover:bg-acid/10 disabled:opacity-40">
            <Sparkles size={11} /> {busy === 'next' ? 'разбирают…' : 'прибрать самое важное'}
          </button>
          <button onClick={() => void run('chron', onChronicle, 'хроника записана')} disabled={disabled || !!busy} className="flex items-center gap-1 rounded border border-line px-2 py-1 font-mono text-[11px] text-zinc-300 hover:border-acid hover:text-acid disabled:opacity-40">
            <BookOpen size={11} /> {busy === 'chron' ? 'пишут…' : h.today_diary ? 'переписать хронику дня' : 'написать хронику дня'}
          </button>
          {h.today_diary && (
            <button onClick={() => onSelect(h.today_diary!)} className="rounded border border-line px-2 py-1 font-mono text-[11px] text-zinc-400 hover:text-zinc-100">
              открыть хронику
            </button>
          )}
        </div>
        {msg && <div className="mt-1.5 font-mono text-[10px] text-acid">{msg}</div>}
        <div className="mt-1.5 text-[10px] text-zinc-600">Когда ты молчишь, каждый третий пульс комнаты уходит на заботу о библиотеке; файлы разбираются сами через пару минут после загрузки; хроника пишется вечером.</div>
      </section>

      <section>
        <div className="mb-1.5 flex items-center gap-2 font-mono text-[10px] uppercase tracking-widest text-zinc-500">
          <Zap size={11} /> автоматы · {rules.length}
          <button
            onClick={() => void run('rules', () => api.runRules().then((r) => setMsg(r.fired.length ? r.fired.join(' · ') : 'ни одно правило не сработало')), 'проверено')}
            disabled={disabled || !!busy || rules.length === 0}
            className="ml-auto rounded border border-line px-1.5 py-0.5 font-mono text-[9px] normal-case tracking-normal text-zinc-400 hover:border-acid hover:text-acid disabled:opacity-40"
            title="Проверить все правила сейчас"
          >
            {busy === 'rules' ? '…' : 'проверить все'}
          </button>
        </div>
        {rules.length === 0 && (
          <div className="text-[11px] text-zinc-600">
            Правил нет. Заведи объект типа <span className="text-[#fcd34d]">⚙ Правило</span>: «когда … → сделать …», и двор будет выполнять его сам.
          </div>
        )}
        <div className="space-y-1">
          {rules.map((r) => {
            const active = r.props.active !== false
            return (
              <div key={r.id} className={`flex items-center gap-2 rounded border border-line/60 px-2 py-1 text-[11px] ${active ? '' : 'opacity-50'}`}>
                <button onClick={() => onSelect(r.id)} className="min-w-0 flex-1 text-left">
                  <div className="truncate text-zinc-200">{r.label}</div>
                  <div className="truncate font-mono text-[9px] text-zinc-500">
                    {String(r.props.when ?? '—')} → {String(r.props.do ?? 'сказать')}
                    {typeof r.props.log === 'string' && r.props.log ? ` · ${r.props.log}` : ''}
                  </div>
                </button>
                <span className="shrink-0 font-mono text-[9px] text-zinc-600">×{Number(r.props.runs ?? 0)}</span>
                <button
                  onClick={() => void run(`rule:${r.id}`, () => api.runRule(r.id).then((x) => setMsg(x.result)), 'выполнено')}
                  disabled={disabled || !!busy}
                  className="shrink-0 rounded border border-[#fcd34d]/50 px-1.5 py-0.5 font-mono text-[9px] text-[#fcd34d] hover:bg-[#fcd34d]/10 disabled:opacity-40"
                  title="Выполнить сейчас"
                >
                  {busy === `rule:${r.id}` ? '…' : '▶'}
                </button>
              </div>
            )
          })}
        </div>
      </section>

      <section>
        <div className="mb-1.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">что сделали жители · {st.recent.length}</div>
        {st.recent.length === 0 && <div className="text-[11px] text-zinc-600">Пока ничего — они ещё присматриваются.</div>}
        <div className="space-y-1">
          {st.recent.map((r, i) => (
            <button key={i} onClick={() => r.object_id && onSelect(r.object_id)} className="flex w-full items-baseline gap-2 rounded px-1 py-0.5 text-left text-[11px] hover:bg-zinc-900">
              <span className="shrink-0 font-mono text-[10px]" style={{ color: AGENT_COLORS[r.agent] ?? '#888' }}>
                {name(r.agent)}
              </span>
              <span className="truncate text-zinc-300">{r.text}</span>
              <span className="ml-auto shrink-0 font-mono text-[9px] text-zinc-600">{new Date(r.at).toLocaleDateString('ru-RU', { day: '2-digit', month: '2-digit' })}</span>
            </button>
          ))}
        </div>
      </section>
    </div>
  )
}
