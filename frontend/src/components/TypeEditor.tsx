import { useEffect, useState } from 'react'
import { ArrowDown, ArrowUp, Plus, Trash2, X } from 'lucide-react'
import type { ObjectType, PropSchema } from '../types'
import { PROP_KINDS } from '../types'

interface Props {
  open: boolean
  types: ObjectType[]
  initialId?: string | null
  onClose: () => void
  onSave: (id: string, patch: { name?: string; icon?: string; color?: string; schema?: { props: PropSchema[]; template?: string } }) => void
  onCreate: (name: string) => void
  onDelete: (id: string) => void
  counts: Record<string, number>
}

const PALETTE = ['#CCFF00', '#00F0FF', '#FF0055', '#f0abfc', '#fbbf24', '#c084fc', '#fb7185', '#fdba74', '#a3e635', '#7dd3fc', '#86efac', '#fde68a', '#67e8f9', '#f9a8d4', '#5eead4', '#c9c9d1']
const ICONS = ['📝', '📄', '☐', '◆', '◯', '🔗', '✦', '◎', '📖', '🎬', '🍳', '📓', '❝', '📅', '📍', '📎', '🖼', '📡', '🎵', '🎮', '💊', '🏋️', '✈️', '💡', '🧪', '🛠', '💰', '🐈', '🌱', '⚡']

function slug(s: string) {
  return s
    .toLowerCase()
    .replace(/[^a-zа-яё0-9]+/gi, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 32)
}

/** Type designer: name, icon, color, note template and the property schema (kinds, options, order). */
export function TypeEditor({ open, types, initialId, onClose, onSave, onCreate, onDelete, counts }: Props) {
  const [sel, setSel] = useState<string | null>(initialId ?? null)
  const [name, setName] = useState('')
  const [icon, setIcon] = useState('')
  const [color, setColor] = useState('')
  const [template, setTemplate] = useState('')
  const [props, setProps] = useState<PropSchema[]>([])
  const [dirty, setDirty] = useState(false)
  const [newName, setNewName] = useState('')

  const t = types.find((x) => x.id === sel) ?? null
  useEffect(() => {
    if (open) setSel(initialId ?? types[0]?.id ?? null)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, initialId])
  useEffect(() => {
    if (!t) return
    setName(t.name)
    setIcon(t.icon)
    setColor(t.color)
    setTemplate(t.schema.template ?? '')
    setProps(t.schema.props.map((p) => ({ ...p })))
    setDirty(false)
  }, [t])

  if (!open) return null

  const mark = <T,>(fn: (v: T) => void) => (v: T) => {
    fn(v)
    setDirty(true)
  }
  const setProp = (i: number, patch: Partial<PropSchema>) => {
    setProps((ps) => ps.map((p, j) => (j === i ? { ...p, ...patch } : p)))
    setDirty(true)
  }
  const move = (i: number, d: -1 | 1) => {
    setProps((ps) => {
      const n = [...ps]
      const j = i + d
      if (j < 0 || j >= n.length) return ps
      ;[n[i], n[j]] = [n[j], n[i]]
      return n
    })
    setDirty(true)
  }
  const save = () => {
    if (!t) return
    const cleaned = props.filter((p) => p.key.trim() && p.label.trim()).map((p) => ({ ...p, key: slug(p.key) || slug(p.label), options: p.kind === 'select' || p.kind === 'multiselect' ? (p.options ?? []).filter(Boolean) : undefined, to: p.kind === 'relation' ? p.to || undefined : undefined, max: p.kind === 'rating' ? p.max ?? 5 : undefined }))
    onSave(t.id, { name: name.trim() || undefined, icon: icon.trim() || undefined, color: color || undefined, schema: { props: cleaned, template: template.trim() ? template : undefined } })
    setDirty(false)
  }
  const inp = 'rounded border border-line bg-panel px-2 py-1 text-xs text-zinc-100 focus:border-acid focus:outline-none'

  return (
    <div className="fixed inset-0 z-50 flex items-end justify-center bg-black/70 sm:items-center sm:p-4" onClick={onClose}>
      <div className="flex h-[92vh] w-full max-w-3xl flex-col overflow-hidden rounded-t-xl border border-line bg-void shadow-2xl sm:h-[80vh] sm:rounded-xl" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center gap-2 border-b border-line px-4 py-2">
          <h2 className="font-mono text-sm font-bold tracking-widest text-zinc-100">ТИПЫ ОБЪЕКТОВ</h2>
          <span className="font-mono text-[10px] text-zinc-500">поля, рейтинги, шаблон заметки — агенты видят схему и заполняют её</span>
          <button onClick={onClose} className="ml-auto rounded p-1 text-zinc-500 hover:text-zinc-200" aria-label="Закрыть">
            <X size={16} />
          </button>
        </div>
        <div className="flex min-h-0 flex-1 flex-col sm:flex-row">
          <div className="flex items-center gap-1 border-b border-line px-2 py-1.5 sm:hidden">
            <select value={sel ?? ''} onChange={(e) => setSel(e.target.value || null)} className={`${inp} flex-1`}>
              {types.map((x) => (
                <option key={x.id} value={x.id}>
                  {x.icon} {x.name} · {counts[x.id] ?? 0}
                </option>
              ))}
            </select>
            <input
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && newName.trim()) {
                  onCreate(newName.trim())
                  setNewName('')
                }
              }}
              placeholder="новый…"
              className={`${inp} w-24`}
            />
          </div>
          <aside className="hidden w-48 shrink-0 overflow-y-auto border-r border-line sm:block">
            {types.map((x) => (
              <button key={x.id} onClick={() => setSel(x.id)} className={`flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs hover:bg-zinc-900 ${sel === x.id ? 'bg-zinc-900 text-zinc-100' : 'text-zinc-400'}`}>
                <span style={{ color: x.color }}>{x.icon}</span>
                <span className="truncate">{x.name}</span>
                <span className="ml-auto font-mono text-[9px] text-zinc-600">{counts[x.id] ?? 0}</span>
              </button>
            ))}
            <div className="flex items-center gap-1 border-t border-line p-2">
              <input
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && newName.trim()) {
                    onCreate(newName.trim())
                    setNewName('')
                  }
                }}
                placeholder="новый тип…"
                className={`${inp} min-w-0 flex-1`}
              />
              <button
                onClick={() => {
                  if (newName.trim()) {
                    onCreate(newName.trim())
                    setNewName('')
                  }
                }}
                className="rounded border border-acid/60 p-1 text-acid"
                title="Создать тип"
              >
                <Plus size={12} />
              </button>
            </div>
          </aside>
          {t ? (
            <div className="min-w-0 flex-1 space-y-4 overflow-y-auto p-4">
              <div className="grid grid-cols-1 items-center gap-x-3 gap-y-2 sm:grid-cols-[auto_1fr]">
                <span className="font-mono text-[10px] uppercase tracking-widest text-zinc-500">название</span>
                <input value={name} onChange={(e) => mark(setName)(e.target.value)} className={inp} />
                <span className="font-mono text-[10px] uppercase tracking-widest text-zinc-500">иконка</span>
                <div className="flex flex-wrap items-center gap-1">
                  <input value={icon} onChange={(e) => mark(setIcon)(e.target.value)} className={`${inp} w-14 text-center`} />
                  {ICONS.map((i) => (
                    <button key={i} onClick={() => mark(setIcon)(i)} className={`rounded px-1 text-sm hover:bg-zinc-800 ${icon === i ? 'bg-zinc-800' : ''}`}>
                      {i}
                    </button>
                  ))}
                </div>
                <span className="font-mono text-[10px] uppercase tracking-widest text-zinc-500">цвет</span>
                <div className="flex flex-wrap items-center gap-1">
                  {PALETTE.map((c) => (
                    <button key={c} onClick={() => mark(setColor)(c)} className={`h-5 w-5 rounded-full border-2 ${color === c ? 'border-white' : 'border-transparent'}`} style={{ background: c }} aria-label={c} />
                  ))}
                  <input value={color} onChange={(e) => mark(setColor)(e.target.value)} className={`${inp} w-20 font-mono`} />
                </div>
              </div>

              <section>
                <div className="mb-1 flex items-center gap-2 font-mono text-[10px] uppercase tracking-widest text-zinc-500">
                  поля · {props.length}
                  <button
                    onClick={() => {
                      setProps((ps) => [...ps, { key: '', label: '', kind: 'text' }])
                      setDirty(true)
                    }}
                    className="ml-auto flex items-center gap-1 rounded border border-acid/60 px-1.5 py-0.5 text-[10px] text-acid hover:bg-acid/10"
                  >
                    <Plus size={10} /> поле
                  </button>
                </div>
                <div className="space-y-1">
                  {props.map((p, i) => (
                    <div key={i} className="grid grid-cols-[1fr_auto] items-center gap-1 rounded border border-line/60 bg-panel/40 p-1 sm:grid-cols-[1fr_1fr_120px_auto]">
                      <input value={p.label} onChange={(e) => setProp(i, { label: e.target.value, key: p.key || slug(e.target.value) })} placeholder="Подпись" className={inp} />
                      <input value={p.key} onChange={(e) => setProp(i, { key: e.target.value })} placeholder="key" className={`${inp} font-mono`} />
                      <select value={p.kind} onChange={(e) => setProp(i, { kind: e.target.value })} className={inp}>
                        {PROP_KINDS.map((k) => (
                          <option key={k.id} value={k.id}>
                            {k.label}
                          </option>
                        ))}
                      </select>
                      <div className="flex items-center gap-0.5">
                        <button onClick={() => move(i, -1)} className="rounded p-0.5 text-zinc-500 hover:text-zinc-200" title="Выше">
                          <ArrowUp size={12} />
                        </button>
                        <button onClick={() => move(i, 1)} className="rounded p-0.5 text-zinc-500 hover:text-zinc-200" title="Ниже">
                          <ArrowDown size={12} />
                        </button>
                        <button
                          onClick={() => {
                            setProps((ps) => ps.filter((_, j) => j !== i))
                            setDirty(true)
                          }}
                          className="rounded p-0.5 text-zinc-500 hover:text-magenta"
                          title="Убрать поле"
                        >
                          <Trash2 size={12} />
                        </button>
                      </div>
                      {(p.kind === 'select' || p.kind === 'multiselect') && (
                        <input value={(p.options ?? []).join(', ')} onChange={(e) => setProp(i, { options: e.target.value.split(',').map((s) => s.trim()) })} placeholder="варианты через запятую" className={`${inp} col-span-2 sm:col-span-4`} />
                      )}
                      {p.kind === 'relation' && (
                        <select value={p.to ?? ''} onChange={(e) => setProp(i, { to: e.target.value || undefined })} className={`${inp} col-span-2 sm:col-span-4`}>
                          <option value="">любой объект</option>
                          {types.map((x) => (
                            <option key={x.id} value={x.id}>
                              только {x.name}
                            </option>
                          ))}
                        </select>
                      )}
                      {p.kind === 'rating' && (
                        <label className="col-span-2 flex items-center gap-2 font-mono text-[10px] text-zinc-500 sm:col-span-4">
                          максимум
                          <input type="number" min={2} max={10} value={p.max ?? 5} onChange={(e) => setProp(i, { max: Number(e.target.value) })} className={`${inp} w-16`} />
                        </label>
                      )}
                    </div>
                  ))}
                  {props.length === 0 && <div className="text-[11px] text-zinc-600">Полей нет — только заголовок и заметка.</div>}
                </div>
              </section>

              <section>
                <div className="mb-1 font-mono text-[10px] uppercase tracking-widest text-zinc-500">шаблон заметки (новые объекты этого типа начинаются с него)</div>
                <textarea value={template} onChange={(e) => mark(setTemplate)(e.target.value)} rows={5} placeholder={'## О чём\n\n## Что взял\n'} className={`${inp} w-full resize-y font-mono`} />
              </section>

              <div className="flex items-center gap-2 border-t border-line pt-3">
                <button onClick={save} disabled={!dirty} className="rounded bg-acid px-4 py-1.5 font-mono text-xs font-bold text-void hover:shadow-glow disabled:opacity-40">
                  Сохранить
                </button>
                <span className="font-mono text-[10px] text-zinc-600">
                  {t.builtin ? 'встроенный тип — поля можно менять' : `тип завёл ${t.created_by}`} · id <code>{t.id}</code>
                </span>
                {!['note', 'page', 'task', 'file', 'image'].includes(t.id) && (
                  <button
                    onClick={() => {
                      if (confirm(`Удалить тип «${t.name}»? Его объекты (${counts[t.id] ?? 0}) станут заметками.`)) {
                        onDelete(t.id)
                        setSel(null)
                      }
                    }}
                    className="ml-auto flex items-center gap-1 rounded border border-magenta/50 px-2 py-1 font-mono text-[10px] text-magenta hover:bg-magenta/10"
                  >
                    <Trash2 size={11} /> удалить тип
                  </button>
                )}
              </div>
            </div>
          ) : (
            <div className="flex-1 p-6 font-mono text-xs text-zinc-600">Выбери тип слева или заведи новый.</div>
          )}
        </div>
      </div>
    </div>
  )
}
