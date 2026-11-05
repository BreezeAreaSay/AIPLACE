import { useEffect, useMemo, useRef, useState } from 'react'
import { Download, FolderInput, Plus, Rss, Search, SlidersHorizontal } from 'lucide-react'
import type { Agent, GraphNode, ObjectType } from '../types'
import { AGENT_COLORS } from '../types'
import { api } from '../api'

interface Props {
  nodes: GraphNode[]
  types: ObjectType[]
  agents: Agent[]
  selectedId: string | null
  /** Search text pushed from outside (e.g. `/найти` in the composer). */
  externalQuery?: { q: string; nonce: number } | null
  onSelect: (id: string) => void
  onCreate: (label: string, typeId: string) => void
  onCreateType: (name: string) => void
  onImport: (files: { name: string; content: string }[]) => Promise<{ imported: number; updated: number; linked: number }>
  onUploadFiles: (files: File[]) => Promise<{ ids: string[] }>
  onEditTypes: (id?: string) => void
  onPollFeeds: () => Promise<{ created: number }>
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
    if (p.kind === 'rating') {
      const n = Number(v) || 0
      if (n > 0) parts.push('★'.repeat(n) + '☆'.repeat(Math.max(0, (p.max ?? 5) - n)))
      continue
    }
    if (p.kind === 'tags' || p.kind === 'multiselect') {
      if (Array.isArray(v) && v.length) parts.push(v.map((x) => `#${x}`).join(' '))
      continue
    }
    if (p.kind === 'image' || p.kind === 'longtext') continue
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

async function readFiles(list: FileList | File[]): Promise<{ name: string; content: string }[]> {
  const files = Array.from(list).filter((f) => /\.(md|markdown|txt)$/i.test(f.name))
  return Promise.all(files.map(async (f) => ({ name: (f as File & { webkitRelativePath?: string }).webkitRelativePath || f.name, content: await f.text() })))
}

/** Anytype-style browser: objects grouped by type, searchable, with key properties; `.md` import by drop. */
export function Library({ nodes, types, agents, selectedId, externalQuery, onSelect, onCreate, onCreateType, onImport, onUploadFiles, onEditTypes, onPollFeeds }: Props) {
  const [typeId, setTypeId] = useState<string>('all')
  const [q, setQ] = useState('')
  const [adding, setAdding] = useState(false)
  const [newTitle, setNewTitle] = useState('')
  const [newType, setNewType] = useState('note')
  const [drag, setDrag] = useState(false)
  const [toast, setToast] = useState<string | null>(null)
  const fileInput = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (externalQuery) {
      setQ(externalQuery.q)
      setTypeId('all')
    }
  }, [externalQuery])

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
        .sort((a, b) => b.pinned - a.pinned || b.updated_at.localeCompare(a.updated_at)),
    [live, typeId, ql],
  )
  const tmap = useMemo(() => Object.fromEntries(types.map((t) => [t.id, t])), [types])
  const name = (id: string) => (id === 'user' ? 'ты' : id === 'archivist' ? 'архивариус' : id === 'mcp' ? 'mcp' : agents.find((a) => a.id === id)?.name.split(' ')[0] ?? id)

  const doImport = async (list: FileList | File[]) => {
    const all = Array.from(list)
    const mdFiles = await readFiles(all.filter((f) => /\.(md|markdown)$/i.test(f.name)))
    const others = all.filter((f) => !/\.(md|markdown)$/i.test(f.name))
    if (!mdFiles.length && !others.length) return
    setToast(`гружу ${all.length}…`)
    const parts: string[] = []
    try {
      if (mdFiles.length) {
        const r = await onImport(mdFiles)
        parts.push(`+${r.imported} заметок · ${r.updated} обновлено · ${r.linked} связей по [[ссылкам]]`)
      }
      if (others.length) {
        const r = await onUploadFiles(others)
        parts.push(`+${r.ids.length} файлов как объекты`)
      }
      setToast(parts.join(' · '))
    } catch (e) {
      setToast(`ошибка: ${(e as Error).message}`)
    }
    window.setTimeout(() => setToast(null), 4000)
  }
  const feeds = live.filter((n) => n.type_id === 'feed').length

  return (
    <div
      className={`relative flex h-full min-h-0 flex-col ${drag ? 'ring-2 ring-inset ring-acid' : ''}`}
      onDragOver={(e) => {
        if (e.dataTransfer.types.includes('Files')) {
          e.preventDefault()
          setDrag(true)
        }
      }}
      onDragLeave={() => setDrag(false)}
      onDrop={(e) => {
        e.preventDefault()
        setDrag(false)
        void doImport(e.dataTransfer.files)
      }}
    >
      <div className="flex items-center gap-2 border-b border-line px-2 py-1.5">
        <div className="flex flex-1 items-center gap-1.5 rounded border border-line bg-panel px-2 py-1">
          <Search size={12} className="text-zinc-500" />
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="поиск по библиотеке" className="w-full bg-transparent font-mono text-[11px] text-zinc-200 placeholder:text-zinc-600 focus:outline-none" />
          {q && (
            <button onClick={() => setQ('')} className="text-zinc-600 hover:text-zinc-200" aria-label="Очистить">
              ×
            </button>
          )}
        </div>
        <button onClick={() => setAdding((v) => !v)} className="flex items-center gap-1 rounded border border-acid/60 px-2 py-1 font-mono text-[11px] text-acid hover:bg-acid/10" title="Новый объект">
          <Plus size={12} />
        </button>
        <button onClick={() => fileInput.current?.click()} className="rounded border border-line p-1 text-zinc-400 hover:border-acid hover:text-acid" title="Загрузить: .md станут заметками, картинки, PDF и прочее — объектами с файлом. Можно перетащить сюда.">
          <FolderInput size={13} />
        </button>
        <button onClick={() => onEditTypes()} className="rounded border border-line p-1 text-zinc-400 hover:border-acid hover:text-acid" title="Типы: поля, рейтинги, шаблоны">
          <SlidersHorizontal size={13} />
        </button>
        {feeds > 0 && (
          <button
            onClick={() => {
              setToast('опрашиваю ленты…')
              void onPollFeeds()
                .then((r) => setToast(`ленты: +${r.created} закладок`))
                .catch((e) => setToast(`ошибка: ${(e as Error).message}`))
                .finally(() => window.setTimeout(() => setToast(null), 3000))
            }}
            className="rounded border border-line p-1 text-zinc-400 hover:border-acid hover:text-acid"
            title={`Опросить RSS-ленты сейчас (${feeds})`}
          >
            <Rss size={13} />
          </button>
        )}
        <a href={api.exportUrl()} download className="rounded border border-line p-1 text-zinc-400 hover:border-acid hover:text-acid" title="Экспорт всей библиотеки в .md">
          <Download size={13} />
        </a>
        <input
          ref={fileInput}
          type="file"
          multiple
          accept="*/*"
          className="hidden"
          onChange={(e) => {
            if (e.target.files) void doImport(e.target.files)
            e.target.value = ''
          }}
        />
      </div>
      {toast && <div className="border-b border-acid/30 bg-acid/5 px-3 py-1 font-mono text-[10px] text-acid">{toast}</div>}
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
              onDoubleClick={() => onEditTypes(t.id)}
              title={`${t.builtin ? t.name : `${t.name} — тип завёл ${name(t.created_by)}`} · двойной клик — настроить поля`}
            >
              {t.icon} {t.name} · {counts[t.id] ?? 0}
              {!t.builtin && <span className="ml-1 text-[8px] text-acid">new</span>}
            </button>
          ))}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {list.length === 0 && (
          <div className="px-4 py-6 text-center font-mono text-[11px] text-zinc-600">
            {live.length === 0 ? 'Библиотека пуста. Просто разговаривай — Архивариус заведёт людей, задачи, идеи и ссылки сам. Или перетащи сюда папку .md-заметок.' : 'ничего не найдено'}
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
                {(preview || n.body) && <div className="truncate text-[11px] text-zinc-500">{preview || n.body.replace(/[#*`>_[\]]/g, '')}</div>}
              </div>
            </button>
          )
        })}
      </div>
      {drag && (
        <div className="pointer-events-none absolute inset-0 flex items-center justify-center bg-void/80 font-mono text-sm text-acid">
          отпусти — .md станут заметками, остальное — объектами с файлом
        </div>
      )}
    </div>
  )
}
