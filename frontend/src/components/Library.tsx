import { useMemo, useState } from 'react'
import { Plus, Search } from 'lucide-react'
import type { Agent, GraphNode, ObjectType } from '../types'
import { AGENT_COLORS } from '../types'

interface Props {
  nodes: GraphNode[]
  types: ObjectType[]
  agents: Agent[]
  selectedId: string | null
  onSelect: (id: string) => void
  onCreate: (label: string, typeId: string) => void
  onCreateType: (name: string) => void
}

function propPreview(n: GraphNode, t: ObjectType | undefined): string {
  const schema = t?.schema.props ?? []
  const parts: string[] = []
  for (const p of schema) {
    const v = n.props[p.key]
    if (v === undefined || v === null || v === '') continue
    if (p.kind === 'checkbox') {
      if (v) parts.push(p.label.toLowerCase())
      continue
    }
    if (p.kind === 'date' && typeof v === 'string') {
      const d = new Date(v)
      parts.push(`${p.label.toLowerCase()}: ${isNaN(d.getTime()) ? v : d.toLocaleDateString('ru-RU', { day: '2-digit', month: '2-digit' })}`)
      continue
    }
    parts.push(`${p.label.toLowerCase()}: ${String(v)}`)
    if (parts.length >= 3) break
  }
  return parts.join(' · ')
}

/** Anytype-style browser: objects grouped by type, searchable, with key properties. */
export function Library({ nodes, types, agents, selectedId, onSelect, onCreate, onCreateType }: Props) {
  const [typeId, setTypeId] = useState<string>('all')
  const [q, setQ] = useState('')
  const [adding, setAdding] = useState(false)
  const [newTitle, setNewTitle] = useState('')
  const [newType, setNewType] = useState('note')

  const live = useMemo(() => nodes.filter((n) => n.archived === 0), [nodes])
  const counts = useMemo(() => {
    const m: Record<string, number> = {}
    for (const n of live) m[n.type_id] = (m[n.type_id] ?? 0) + 1
    return m
  }, [live])
  const ql = q.trim().toLowerCase()
  const list = useMemo(
    () =>
      live
        .filter((n) => (typeId === 'all' || n.type_id === typeId) && (!ql || n.label.toLowerCase().includes(ql) || n.body.toLowerCase().includes(ql) || JSON.stringify(n.props).toLowerCase().includes(ql)))
        .sort((a, b) => (b.pinned - a.pinned) || b.updated_at.localeCompare(a.updated_at)),
    [live, typeId, ql],
  )
  const tmap = useMemo(() => Object.fromEntries(types.map((t) => [t.id, t])), [types])
  const name = (id: string) => (id === 'user' ? 'ты' : id === 'archivist' ? 'архивариус' : agents.find((a) => a.id === id)?.name.split(' ')[0] ?? id)

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex items-center gap-2 border-b border-line px-2 py-1.5">
        <div className="flex flex-1 items-center gap-1.5 rounded border border-line bg-panel px-2 py-1">
          <Search size={12} className="text-zinc-500" />
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="поиск по библиотеке" className="w-full bg-transparent font-mono text-[11px] text-zinc-200 placeholder:text-zinc-600 focus:outline-none" />
        </div>
        <button onClick={() => setAdding((v) => !v)} className="flex items-center gap-1 rounded border border-acid/60 px-2 py-1 font-mono text-[11px] text-acid hover:bg-acid/10" title="Новый объект">
          <Plus size={12} />
        </button>
      </div>
      {adding && (
        <div className="flex items-center gap-1.5 border-b border-line bg-panel/60 px-2 py-1.5">
          <select value={newType} onChange={(e) => setNewType(e.target.value)} className="rounded border border-line bg-void px-1 py-1 font-mono text-[11px] text-zinc-300">
            {types.map((t) => (
              <option key={t.id} value={t.id}>
                {t.icon} {t.name}
              </option>
            ))}
          </select>
          <input
            autoFocus
            value={newTitle}
            onChange={(e) => setNewTitle(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && newTitle.trim()) {
                onCreate(newTitle.trim(), newType)
                setNewTitle('')
                setAdding(false)
              }
              if (e.key === 'Escape') setAdding(false)
            }}
            placeholder="название… (Enter)"
            className="flex-1 rounded border border-line bg-void px-2 py-1 text-xs text-zinc-100 placeholder:text-zinc-600 focus:border-acid focus:outline-none"
          />
          <button
            onClick={() => {
              const n = prompt('Название нового типа (свойства можно добавить потом):')
              if (n && n.trim()) onCreateType(n.trim())
            }}
            className="rounded border border-line px-1.5 py-1 font-mono text-[10px] text-zinc-500 hover:text-zinc-200"
            title="Завести новый тип"
          >
            + тип
          </button>
        </div>
      )}
      <div className="flex gap-1 overflow-x-auto border-b border-line px-2 py-1.5 font-mono text-[10px]">
        <button onClick={() => setTypeId('all')} className={`shrink-0 rounded px-2 py-0.5 ${typeId === 'all' ? 'bg-zinc-800 text-zinc-100' : 'text-zinc-500 hover:text-zinc-200'}`}>
          все · {live.length}
        </button>
        {types
          .filter((t) => (counts[t.id] ?? 0) > 0 || typeId === t.id)
          .map((t) => (
            <button
              key={t.id}
              onClick={() => setTypeId(t.id)}
              className={`shrink-0 rounded px-2 py-0.5 ${typeId === t.id ? 'bg-zinc-800 text-zinc-100' : 'text-zinc-500 hover:text-zinc-200'}`}
              style={typeId === t.id ? { color: t.color } : undefined}
              title={t.builtin ? t.name : `${t.name} — тип завёл ${name(t.created_by)}`}
            >
              {t.icon} {t.name} · {counts[t.id] ?? 0}
              {!t.builtin && <span className="ml-1 text-[8px] text-acid">new</span>}
            </button>
          ))}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {list.length === 0 && (
          <div className="px-4 py-6 text-center font-mono text-[11px] text-zinc-600">
            {live.length === 0 ? 'Библиотека пуста. Просто разговаривай — Архивариус заведёт людей, задачи, идеи и ссылки сам.' : 'ничего не найдено'}
          </div>
        )}
        {list.map((n) => {
          const t = tmap[n.type_id]
          const preview = propPreview(n, t)
          return (
            <button
              key={n.id}
              onClick={() => onSelect(n.id)}
              className={`flex w-full items-start gap-2 border-b border-line/60 px-3 py-2 text-left hover:bg-zinc-900/60 ${selectedId === n.id ? 'bg-zinc-900' : ''}`}
            >
              <span className="mt-0.5 w-5 shrink-0 text-center text-sm" style={{ color: t?.color }} title={t?.name}>
                {t?.icon ?? '◇'}
              </span>
              <div className="min-w-0 flex-1">
                <div className="flex items-baseline gap-2">
                  <span className={`truncate text-[13px] ${n.type_id === 'task' && n.props.done ? 'text-zinc-500 line-through' : 'text-zinc-100'}`}>{n.label}</span>
                  {n.pinned !== 0 && <span className="text-[9px] text-zinc-500">📌</span>}
                  <span className="ml-auto shrink-0 font-mono text-[9px]" style={{ color: AGENT_COLORS[n.created_by] ?? '#666' }}>
                    {name(n.created_by)}
                  </span>
                </div>
                {(preview || n.body) && <div className="truncate text-[11px] text-zinc-500">{preview || n.body.replace(/[#*`>_]/g, '')}</div>}
              </div>
            </button>
          )
        })}
      </div>
    </div>
  )
}
