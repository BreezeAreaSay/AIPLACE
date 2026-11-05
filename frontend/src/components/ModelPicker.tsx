import { useEffect, useMemo, useRef, useState } from 'react'
import { ChevronDown } from 'lucide-react'

export interface ModelInfo {
  id: string
  name: string
  context_length: number
  prompt_price: number
  completion_price: number
  free: boolean
  json_mode: boolean
}

interface Props {
  value: string
  onChange: (id: string) => void
  models: ModelInfo[]
  /** Catalog state: loading | ready | error. */
  status?: 'loading' | 'ready' | 'error'
  disabled?: boolean
  placeholder?: string
  /** Hint which models suit this role (e.g. JSON support for the Director). */
  preferJson?: boolean
}

function fmt(v: number): string {
  const s = v >= 1 ? v.toFixed(2) : v.toFixed(3)
  return s.replace(/\.?0+$/, '')
}

function price(m: ModelInfo) {
  if (m.free) return 'free'
  return `$${fmt(m.prompt_price)} / $${fmt(m.completion_price)}`
}

/** Searchable combobox over the OpenRouter catalog; free-tier first, manual ids allowed. */
export function ModelPicker({ value, onChange, models, status = 'ready', disabled, placeholder, preferJson }: Props) {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const [onlyFree, setOnlyFree] = useState(false)
  const box = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const onDoc = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', onDoc)
    return () => document.removeEventListener('mousedown', onDoc)
  }, [])

  const q = query.trim().toLowerCase()
  const list = useMemo(
    () =>
      models
        .filter((m) => (!onlyFree || m.free) && (!q || m.id.toLowerCase().includes(q) || m.name.toLowerCase().includes(q)))
        .slice(0, 80),
    [models, onlyFree, q],
  )
  const current = models.find((m) => m.id === value)

  return (
    <div ref={box} className="relative">
      <div className="flex items-center gap-1 rounded border border-line bg-panel px-2 py-1.5 focus-within:border-acid">
        <input
          value={open ? query : value}
          disabled={disabled}
          onFocus={() => {
            setQuery('')
            setOpen(true)
          }}
          onChange={(e) => {
            setQuery(e.target.value)
            setOpen(true)
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && query.trim()) {
              onChange(query.trim())
              setOpen(false)
            }
            if (e.key === 'Escape') setOpen(false)
          }}
          placeholder={placeholder ?? 'provider/model или поиск…'}
          className="min-w-0 flex-1 bg-transparent font-mono text-xs text-zinc-100 placeholder:text-zinc-600 focus:outline-none disabled:opacity-50"
        />
        {current && !open && (
          <span className={`shrink-0 rounded px-1 font-mono text-[9px] ${current.free ? 'bg-acid/15 text-acid' : 'bg-zinc-800 text-zinc-400'}`}>{price(current)}</span>
        )}
        <button type="button" onClick={() => setOpen((v) => !v)} disabled={disabled} className="shrink-0 text-zinc-500 hover:text-zinc-200">
          <ChevronDown size={14} />
        </button>
      </div>
      {open && !disabled && (
        <div className="absolute z-30 mt-1 max-h-72 w-full overflow-hidden rounded border border-line bg-void shadow-2xl">
          <div className="flex items-center gap-2 border-b border-line px-2 py-1 font-mono text-[10px] text-zinc-500">
            <label className="flex cursor-pointer items-center gap-1">
              <input type="checkbox" checked={onlyFree} onChange={(e) => setOnlyFree(e.target.checked)} className="accent-[#CCFF00]" /> только бесплатные
            </label>
            <span className="ml-auto">
              {models.length ? `${list.length} из ${models.length}` : status === 'error' ? 'каталог недоступен — введи id и Enter' : 'каталог грузится…'}
            </span>
          </div>
          <div className="max-h-60 overflow-y-auto">
            {query.trim() && !models.some((m) => m.id === query.trim()) && (
              <button
                type="button"
                onClick={() => {
                  onChange(query.trim())
                  setOpen(false)
                }}
                className="block w-full px-2 py-1.5 text-left font-mono text-[11px] text-zinc-300 hover:bg-zinc-800"
              >
                использовать «{query.trim()}» как есть
              </button>
            )}
            {list.map((m) => (
              <button
                type="button"
                key={m.id}
                onClick={() => {
                  onChange(m.id)
                  setOpen(false)
                }}
                className={`flex w-full items-center gap-2 px-2 py-1.5 text-left hover:bg-zinc-800 ${m.id === value ? 'bg-zinc-900' : ''}`}
              >
                <div className="min-w-0 flex-1">
                  <div className="truncate text-[12px] text-zinc-100">{m.name}</div>
                  <div className="truncate font-mono text-[10px] text-zinc-500">
                    {m.id}
                    {m.context_length ? ` · ${Math.round(m.context_length / 1000)}k` : ''}
                    {preferJson && !m.json_mode ? ' · без JSON-режима' : ''}
                  </div>
                </div>
                <span className={`shrink-0 rounded px-1 font-mono text-[9px] ${m.free ? 'bg-acid/15 text-acid' : 'bg-zinc-800 text-zinc-400'}`}>{price(m)}</span>
              </button>
            ))}
            {list.length === 0 && models.length > 0 && (
              <div className="px-2 py-2 font-mono text-[11px] text-zinc-600">ничего не найдено — Enter подставит введённый id</div>
            )}
          </div>
        </div>
      )}
    </div>
  )
}
