import type { GraphNode, PropSchema } from '../types'

interface Props {
  schema: PropSchema[]
  values: Record<string, unknown>
  nodes: GraphNode[]
  disabled?: boolean
  onChange: (key: string, value: unknown) => void
}

/** Renders a type's property schema as a compact form (Anytype-style relations panel). */
export function PropsEditor({ schema, values, nodes, disabled, onChange }: Props) {
  if (schema.length === 0) return null
  const base = 'w-full rounded border border-line bg-panel px-2 py-1 text-xs text-zinc-100 focus:border-acid focus:outline-none disabled:opacity-50'
  return (
    <div className="space-y-1.5">
      {schema.map((p) => {
        const v = values[p.key]
        return (
          <label key={p.key} className="grid grid-cols-[96px_1fr] items-center gap-2">
            <span className="truncate font-mono text-[10px] uppercase tracking-wider text-zinc-500" title={p.key}>
              {p.label}
            </span>
            {p.kind === 'checkbox' ? (
              <input type="checkbox" checked={!!v} disabled={disabled} onChange={(e) => onChange(p.key, e.target.checked)} className="h-4 w-4 accent-[#CCFF00]" />
            ) : p.kind === 'select' ? (
              <select value={typeof v === 'string' ? v : ''} disabled={disabled} onChange={(e) => onChange(p.key, e.target.value || null)} className={base}>
                <option value="">—</option>
                {(p.options ?? []).map((o) => (
                  <option key={o} value={o}>
                    {o}
                  </option>
                ))}
              </select>
            ) : p.kind === 'relation' ? (
              <select value={typeof v === 'string' ? v : ''} disabled={disabled} onChange={(e) => onChange(p.key, e.target.value || null)} className={base}>
                <option value="">—</option>
                {nodes
                  .filter((n) => !p.to || n.type_id === p.to)
                  .map((n) => (
                    <option key={n.id} value={n.id}>
                      {n.label}
                    </option>
                  ))}
              </select>
            ) : p.kind === 'date' ? (
              <input
                type="date"
                value={typeof v === 'string' ? v.slice(0, 10) : ''}
                disabled={disabled}
                onChange={(e) => onChange(p.key, e.target.value || null)}
                className={base}
              />
            ) : p.kind === 'number' ? (
              <input type="number" value={typeof v === 'number' ? v : typeof v === 'string' ? v : ''} disabled={disabled} onChange={(e) => onChange(p.key, e.target.value === '' ? null : Number(e.target.value))} className={base} />
            ) : (
              <input
                type={p.kind === 'url' ? 'url' : 'text'}
                value={typeof v === 'string' ? v : v == null ? '' : String(v)}
                disabled={disabled}
                onChange={(e) => onChange(p.key, e.target.value || null)}
                className={base}
              />
            )}
          </label>
        )
      })}
    </div>
  )
}
