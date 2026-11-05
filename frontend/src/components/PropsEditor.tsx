import { useState } from 'react'
import { ExternalLink, Star, X } from 'lucide-react'
import type { GraphNode, PropSchema } from '../types'
import { useLive, useNoteScope } from '../live/LiveContext'
import { evalFormula, formatValue } from '../live/formula'

interface Props {
  schema: PropSchema[]
  values: Record<string, unknown>
  nodes: GraphNode[]
  disabled?: boolean
  onChange: (key: string, value: unknown) => void
  onOpen?: (id: string) => void
}

const base = 'w-full rounded border border-line bg-panel px-2 py-1 text-xs text-zinc-100 focus:border-acid focus:outline-none disabled:opacity-50'

export function Rating({ value, max = 5, disabled, onChange }: { value: number; max?: number; disabled?: boolean; onChange?: (v: number | null) => void }) {
  const [hover, setHover] = useState<number | null>(null)
  const shown = hover ?? value
  return (
    <div className="flex items-center gap-0.5" onMouseLeave={() => setHover(null)}>
      {Array.from({ length: max }, (_, i) => i + 1).map((i) => (
        <button
          key={i}
          type="button"
          disabled={disabled || !onChange}
          onMouseEnter={() => setHover(i)}
          onClick={() => onChange?.(i === value ? null : i)}
          className="disabled:cursor-default"
          title={`${i} из ${max}`}
        >
          <Star size={15} className={i <= shown ? 'fill-[#fbbf24] text-[#fbbf24]' : 'text-zinc-700'} />
        </button>
      ))}
      {value > 0 && <span className="ml-1 font-mono text-[10px] text-zinc-500">{value}/{max}</span>}
    </div>
  )
}

function Chips({ value, options, disabled, onChange, free }: { value: string[]; options?: string[]; disabled?: boolean; onChange: (v: string[]) => void; free?: boolean }) {
  const [draft, setDraft] = useState('')
  const add = (t: string) => {
    const v = t.trim()
    if (!v || value.includes(v)) return
    onChange([...value, v])
    setDraft('')
  }
  return (
    <div className="flex flex-wrap items-center gap-1">
      {value.map((v) => (
        <span key={v} className="inline-flex items-center gap-1 rounded border border-line bg-zinc-900 px-1.5 py-0.5 font-mono text-[10px] text-zinc-200">
          {v}
          {!disabled && (
            <button type="button" onClick={() => onChange(value.filter((x) => x !== v))} className="text-zinc-500 hover:text-magenta" aria-label="убрать">
              <X size={10} />
            </button>
          )}
        </span>
      ))}
      {!disabled &&
        (options ?? [])
          .filter((o) => !value.includes(o))
          .map((o) => (
            <button key={o} type="button" onClick={() => add(o)} className="rounded border border-dashed border-line px-1.5 py-0.5 font-mono text-[10px] text-zinc-500 hover:border-acid hover:text-acid">
              + {o}
            </button>
          ))}
      {free && !disabled && (
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ',') {
              e.preventDefault()
              add(draft)
            }
            if (e.key === 'Backspace' && !draft && value.length) onChange(value.slice(0, -1))
          }}
          onBlur={() => add(draft)}
          placeholder="+ тег"
          className="w-16 bg-transparent font-mono text-[10px] text-zinc-200 placeholder:text-zinc-600 focus:outline-none"
        />
      )}
    </div>
  )
}

function asList(v: unknown): string[] {
  if (Array.isArray(v)) return v.map(String)
  if (typeof v === 'string' && v.trim()) return v.split(',').map((s) => s.trim()).filter(Boolean)
  return []
}

/** Renders a type's property schema as a compact form (Anytype-style relations panel). */
export function PropsEditor({ schema, values, nodes, disabled, onChange, onOpen }: Props) {
  const live = useLive()
  const { nodeId } = useNoteScope()
  if (schema.length === 0) return null
  return (
    <div className="space-y-1.5">
      {schema.map((p) => {
        const v = values[p.key]
        let field: JSX.Element
        switch (p.kind) {
          case 'formula': {
            const node = live?.nodes.find((n) => n.id === nodeId)
            const r = node && live ? evalFormula(p.expr ?? '', { node, nodes: live.nodes, edges: live.edges }) : { value: null, error: 'вне заметки' }
            field = (
              <div className="flex items-center gap-2 text-xs" title={p.expr}>
                <span className="font-mono text-zinc-100">{formatValue(r.value)}</span>
                <span className="font-mono text-[9px] text-zinc-600">ƒ {p.expr}</span>
                {r.error && <span className="font-mono text-[9px] text-magenta">{r.error}</span>}
              </div>
            )
            break
          }
          case 'checkbox':
            field = <input type="checkbox" checked={!!v} disabled={disabled} onChange={(e) => onChange(p.key, e.target.checked)} className="h-4 w-4 accent-[#CCFF00]" />
            break
          case 'rating':
            field = <Rating value={typeof v === 'number' ? v : Number(v) || 0} max={p.max ?? 5} disabled={disabled} onChange={(n) => onChange(p.key, n)} />
            break
          case 'select':
            field = (
              <select value={typeof v === 'string' ? v : ''} disabled={disabled} onChange={(e) => onChange(p.key, e.target.value || null)} className={base}>
                <option value="">—</option>
                {(p.options ?? []).map((o) => (
                  <option key={o} value={o}>
                    {o}
                  </option>
                ))}
                {typeof v === 'string' && v && !(p.options ?? []).includes(v) && <option value={v}>{v}</option>}
              </select>
            )
            break
          case 'multiselect':
            field = <Chips value={asList(v)} options={p.options} disabled={disabled} onChange={(list) => onChange(p.key, list.length ? list : null)} />
            break
          case 'tags':
            field = <Chips value={asList(v)} disabled={disabled} free onChange={(list) => onChange(p.key, list.length ? list : null)} />
            break
          case 'relation': {
            const id = typeof v === 'string' ? v : ''
            field = (
              <div className="flex items-center gap-1">
                <select value={id} disabled={disabled} onChange={(e) => onChange(p.key, e.target.value || null)} className={base}>
                  <option value="">—</option>
                  {nodes
                    .filter((n) => n.archived === 0 && (!p.to || n.type_id === p.to))
                    .map((n) => (
                      <option key={n.id} value={n.id}>
                        {n.label}
                      </option>
                    ))}
                </select>
                {id && onOpen && (
                  <button type="button" onClick={() => onOpen(id)} className="shrink-0 text-zinc-500 hover:text-acid" title="Открыть">
                    <ExternalLink size={12} />
                  </button>
                )}
              </div>
            )
            break
          }
          case 'date':
            field = <input type="date" value={typeof v === 'string' ? v.slice(0, 10) : ''} disabled={disabled} onChange={(e) => onChange(p.key, e.target.value || null)} className={base} />
            break
          case 'datetime':
            field = <input type="datetime-local" value={typeof v === 'string' ? v.slice(0, 16) : ''} disabled={disabled} onChange={(e) => onChange(p.key, e.target.value || null)} className={base} />
            break
          case 'number':
            field = <input type="number" value={typeof v === 'number' ? v : typeof v === 'string' ? v : ''} disabled={disabled} onChange={(e) => onChange(p.key, e.target.value === '' ? null : Number(e.target.value))} className={base} />
            break
          case 'longtext':
            field = <textarea rows={2} value={typeof v === 'string' ? v : v == null ? '' : String(v)} disabled={disabled} onChange={(e) => onChange(p.key, e.target.value || null)} className={`${base} resize-y`} />
            break
          case 'image': {
            const url = typeof v === 'string' ? v : ''
            field = (
              <div className="flex items-start gap-2">
                {url && <img src={url} alt="" className="h-16 w-12 shrink-0 rounded border border-line object-cover" onError={(e) => ((e.target as HTMLImageElement).style.display = 'none')} />}
                <input type="url" value={url} disabled={disabled} onChange={(e) => onChange(p.key, e.target.value || null)} placeholder="https://…" className={base} />
              </div>
            )
            break
          }
          case 'url':
          case 'email':
          case 'phone': {
            const str = typeof v === 'string' ? v : v == null ? '' : String(v)
            const href = p.kind === 'url' ? str : p.kind === 'email' ? `mailto:${str}` : `tel:${str}`
            field = (
              <div className="flex items-center gap-1">
                <input type={p.kind === 'url' ? 'url' : p.kind === 'email' ? 'email' : 'tel'} value={str} disabled={disabled} onChange={(e) => onChange(p.key, e.target.value || null)} className={base} />
                {str && (
                  <a href={href} target={p.kind === 'url' ? '_blank' : undefined} rel="noreferrer" className="shrink-0 text-zinc-500 hover:text-acid" title="Открыть">
                    <ExternalLink size={12} />
                  </a>
                )}
              </div>
            )
            break
          }
          default:
            field = <input type="text" value={typeof v === 'string' ? v : v == null ? '' : String(v)} disabled={disabled} onChange={(e) => onChange(p.key, e.target.value || null)} className={base} />
        }
        return (
          <label key={p.key} className="grid grid-cols-[96px_1fr] items-center gap-2">
            <span className="truncate font-mono text-[10px] uppercase tracking-wider text-zinc-500" title={p.key}>
              {p.label}
            </span>
            {field}
          </label>
        )
      })}
    </div>
  )
}
