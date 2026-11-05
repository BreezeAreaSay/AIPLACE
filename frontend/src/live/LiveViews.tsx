import { useMemo } from 'react'
import type { GraphNode, ObjectType } from '../types'
import { AGENT_COLORS } from '../types'
import type { Query } from './query'
import { useLive } from './LiveContext'
import { Rating } from '../components/PropsEditor'

const fmtDate = (v: unknown) => (typeof v === 'string' && v ? new Date(v).toLocaleDateString('ru-RU', { day: '2-digit', month: '2-digit' }) : '')

function cell(n: GraphNode, key: string, types: ObjectType[]): React.ReactNode {
  if (key === 'label' || key === 'title') return n.label
  if (key === 'type') return types.find((t) => t.id === n.type_id)?.name ?? n.type_id
  if (key === 'author' || key === 'owner') return n.created_by
  if (key === 'updated') return fmtDate(n.updated_at)
  if (key === 'created') return fmtDate(n.created_at)
  const v = n.props[key]
  const schema = types.find((t) => t.id === n.type_id)?.schema.props.find((p) => p.key === key)
  if (schema?.kind === 'rating') return <Rating value={Number(v) || 0} max={schema.max ?? 5} />
  if (schema?.kind === 'checkbox' || typeof v === 'boolean') return v ? '✓' : '·'
  if (schema?.kind === 'date' || schema?.kind === 'datetime') return fmtDate(v)
  if (Array.isArray(v)) return v.map((x) => `#${x}`).join(' ')
  if (v === null || v === undefined || v === '') return <span className="text-zinc-700">—</span>
  return String(v)
}

export function ListView({ items, types }: { items: GraphNode[]; types: ObjectType[] }) {
  const live = useLive()
  return (
    <div className="divide-y divide-line/60">
      {items.map((n) => {
        const t = types.find((x) => x.id === n.type_id)
        const isTask = n.type_id === 'task'
        const due = typeof n.props.due === 'string' ? n.props.due : typeof n.props.date === 'string' ? n.props.date : ''
        const overdue = due && due.slice(0, 10) < new Date().toISOString().slice(0, 10) && n.props.done !== true
        return (
          <div key={n.id} className="flex items-center gap-2 py-1 text-[12px]">
            {isTask ? (
              <input type="checkbox" checked={n.props.done === true} onChange={(e) => live?.actions.patchNode(n.id, { props: { done: e.target.checked, status: e.target.checked ? 'сделана' : 'в работе' } })} className="h-3.5 w-3.5 accent-[#CCFF00]" />
            ) : (
              <span style={{ color: t?.color }}>{t?.icon ?? '◇'}</span>
            )}
            <button onClick={() => live?.actions.openNode(n.id)} className={`min-w-0 flex-1 truncate text-left hover:underline ${n.props.done === true ? 'text-zinc-500 line-through' : 'text-zinc-200'}`}>
              {n.label}
            </button>
            {typeof n.props.rating === 'number' && n.props.rating > 0 && <span className="text-[#fbbf24]">{'★'.repeat(Number(n.props.rating))}</span>}
            {typeof n.props.status === 'string' && n.props.status && <span className="rounded border border-line px-1 font-mono text-[9px] text-zinc-500">{n.props.status}</span>}
            {due && <span className={`font-mono text-[10px] ${overdue ? 'text-magenta' : 'text-zinc-500'}`}>{fmtDate(due)}</span>}
            <span className="font-mono text-[9px]" style={{ color: AGENT_COLORS[n.created_by] ?? '#555' }}>
              {n.created_by === 'user' ? '' : n.created_by}
            </span>
          </div>
        )
      })}
    </div>
  )
}

export function TableView({ items, types, q }: { items: GraphNode[]; types: ObjectType[]; q: Query }) {
  const live = useLive()
  const cols = useMemo(() => {
    if (q.cols?.length) return q.cols
    const t = types.find((x) => x.id === (q.type ?? items[0]?.type_id))
    return ['label', ...(t?.schema.props.filter((p) => p.kind !== 'longtext' && p.key !== 'summary').slice(0, 4).map((p) => p.key) ?? ['type', 'updated'])]
  }, [q, types, items])
  const label = (c: string) => types.flatMap((t) => t.schema.props).find((p) => p.key === c)?.label ?? ({ label: 'Название', type: 'Тип', updated: 'Изменён', created: 'Создан', author: 'Автор' } as Record<string, string>)[c] ?? c
  return (
    <div className="md-table-wrap">
      <table>
        <thead>
          <tr>
            {cols.map((c) => (
              <th key={c}>{label(c)}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {items.map((n) => (
            <tr key={n.id} className="cursor-pointer" onClick={() => live?.actions.openNode(n.id)}>
              {cols.map((c) => (
                <td key={c} className={c === 'label' ? 'text-zinc-100' : 'text-zinc-400'}>
                  {cell(n, c, types)}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

export function KanbanView({ items, types, q }: { items: GraphNode[]; types: ObjectType[]; q: Query }) {
  const live = useLive()
  const key = q.group ?? 'status'
  const t = types.find((x) => x.id === (q.type ?? items[0]?.type_id))
  const schema = t?.schema.props.find((p) => p.key === key)
  const columns: string[] = schema?.options?.length ? [...schema.options] : Array.from(new Set(items.map((n) => String(n.props[key] ?? '')))).filter(Boolean)
  const groups = [...columns, '—']
  // "—" collects only values no column claims (null / "" / anything not in the list)
  const of = (col: string) => items.filter((n) => (col === '—' ? !columns.includes(String(n.props[key] ?? '')) : String(n.props[key] ?? '') === col))
  const move = (n: GraphNode, dir: 1 | -1) => {
    const cur = columns.indexOf(String(n.props[key] ?? ''))
    const next = columns[Math.min(columns.length - 1, Math.max(0, (cur === -1 ? -1 : cur) + dir))]
    if (next) live?.actions.patchNode(n.id, { props: { [key]: next, ...(key === 'status' && n.type_id === 'task' ? { done: next === 'сделана' } : {}) } })
  }
  return (
    <div className="flex gap-2 overflow-x-auto pb-1">
      {groups.map((col) => {
        const list = of(col)
        if (col === '—' && list.length === 0) return null
        return (
          <div key={col} className="w-44 shrink-0 rounded border border-line bg-panel/40">
            <div className="border-b border-line px-2 py-1 font-mono text-[10px] uppercase tracking-wider text-zinc-500">
              {col} <span className="text-zinc-700">· {list.length}</span>
            </div>
            <div className="space-y-1 p-1">
              {list.map((n) => (
                <div key={n.id} className="group rounded border border-line/60 bg-void px-2 py-1 text-[11px]">
                  <button onClick={() => live?.actions.openNode(n.id)} className="block w-full truncate text-left text-zinc-200 hover:underline">
                    {n.label}
                  </button>
                  <div className="mt-0.5 flex items-center gap-1 font-mono text-[9px] text-zinc-600">
                    {typeof n.props.due === 'string' && <span>{fmtDate(n.props.due)}</span>}
                    {typeof n.props.priority === 'string' && <span>{n.props.priority}</span>}
                    {columns.length > 1 && (
                      <span className="ml-auto hidden gap-1 group-hover:flex">
                        <button onClick={() => move(n, -1)} className="hover:text-acid">
                          ←
                        </button>
                        <button onClick={() => move(n, 1)} className="hover:text-acid">
                          →
                        </button>
                      </span>
                    )}
                  </div>
                </div>
              ))}
            </div>
          </div>
        )
      })}
    </div>
  )
}

export function CalendarView({ items, types, q }: { items: GraphNode[]; types: ObjectType[]; q: Query }) {
  const live = useLive()
  const key = q.group ?? 'due'
  const now = new Date()
  const first = new Date(now.getFullYear(), now.getMonth(), 1)
  const start = new Date(first)
  start.setDate(first.getDate() - ((first.getDay() + 6) % 7))
  const days: Date[] = Array.from({ length: 42 }, (_, i) => {
    const d = new Date(start)
    d.setDate(start.getDate() + i)
    return d
  })
  const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
  const byDay = new Map<string, GraphNode[]>()
  for (const n of items) {
    const v = n.props[key] ?? n.props.due ?? n.props.date
    if (typeof v === 'string' && v.length >= 10) byDay.set(v.slice(0, 10), [...(byDay.get(v.slice(0, 10)) ?? []), n])
  }
  const today = iso(now)
  return (
    <div>
      <div className="mb-1 font-mono text-[10px] uppercase tracking-wider text-zinc-500">{now.toLocaleDateString('ru-RU', { month: 'long', year: 'numeric' })}</div>
      <div className="grid grid-cols-7 gap-px rounded border border-line bg-line text-[10px]">
        {['пн', 'вт', 'ср', 'чт', 'пт', 'сб', 'вс'].map((d) => (
          <div key={d} className="bg-panel px-1 py-0.5 text-center font-mono text-zinc-600">
            {d}
          </div>
        ))}
        {days.map((d) => {
          const k = iso(d)
          const list = byDay.get(k) ?? []
          const dim = d.getMonth() !== now.getMonth()
          return (
            <div key={k} className={`min-h-[52px] bg-void p-0.5 ${dim ? 'opacity-40' : ''} ${k === today ? 'ring-1 ring-inset ring-acid' : ''}`}>
              <div className="font-mono text-[9px] text-zinc-600">{d.getDate()}</div>
              {list.slice(0, 3).map((n) => {
                const t = types.find((x) => x.id === n.type_id)
                return (
                  <button key={n.id} onClick={() => live?.actions.openNode(n.id)} className="block w-full truncate rounded px-0.5 text-left text-[9px] hover:bg-zinc-800" style={{ color: t?.color }} title={n.label}>
                    {n.label}
                  </button>
                )
              })}
              {list.length > 3 && <div className="font-mono text-[8px] text-zinc-600">+{list.length - 3}</div>}
            </div>
          )
        })}
      </div>
    </div>
  )
}

export function ChartView({ items, q }: { items: GraphNode[]; q: Query }) {
  const live = useLive()
  const y = q.y ?? 'rating'
  const x = q.x ?? 'label'
  const points = items
    .map((n) => ({ n, y: Number(n.props[y]), x: x === 'label' ? n.label : x === 'created' ? n.created_at.slice(0, 10) : x === 'updated' ? n.updated_at.slice(0, 10) : String(n.props[x] ?? '') }))
    .filter((p) => !isNaN(p.y))
  if (!points.length) return <div className="font-mono text-[10px] text-zinc-600">нет числовых значений «{y}»</div>
  const isTime = /^\d{4}-\d{2}-\d{2}/.test(points[0].x)
  if (isTime) points.sort((a, b) => a.x.localeCompare(b.x))
  const max = Math.max(...points.map((p) => p.y), 1)
  const W = 560
  const H = 140
  const pad = 24
  const bw = Math.max(4, (W - pad * 2) / points.length - 3)
  return (
    <svg viewBox={`0 0 ${W} ${H + 30}`} className="w-full" role="img">
      <line x1={pad} y1={H} x2={W - pad / 2} y2={H} stroke="#2a2b36" />
      {isTime && points.length > 1 && (
        <polyline fill="none" stroke="#00F0FF" strokeWidth={1.5} points={points.map((p, i) => `${pad + i * ((W - pad * 2) / (points.length - 1))},${H - (p.y / max) * (H - 10)}`).join(' ')} />
      )}
      {points.map((p, i) => {
        const px = isTime && points.length > 1 ? pad + i * ((W - pad * 2) / (points.length - 1)) : pad + i * (bw + 3)
        const h = (p.y / max) * (H - 10)
        return (
          <g key={p.n.id} className="cursor-pointer" onClick={() => live?.actions.openNode(p.n.id)}>
            {isTime ? <circle cx={px} cy={H - h} r={3} fill="#CCFF00" /> : <rect x={px} y={H - h} width={bw} height={h} fill="#CCFF00" opacity={0.85} rx={1} />}
            <title>{`${p.n.label}: ${p.y}`}</title>
            {!isTime && points.length <= 16 && (
              <text x={px + bw / 2} y={H + 12} fontSize={8} fill="#8a8ca0" textAnchor="middle">
                {p.x.length > 10 ? p.x.slice(0, 9) + '…' : p.x}
              </text>
            )}
          </g>
        )
      })}
      <text x={pad} y={10} fontSize={9} fill="#5a5c6c">
        {y} · max {max}
      </text>
    </svg>
  )
}
