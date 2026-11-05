import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Bold, CheckSquare, Code, Heading2, Italic, Link as LinkIcon, List, ListOrdered, Quote, Brackets } from 'lucide-react'
import type { GraphNode, ObjectType } from '../types'

interface Props {
  value: string
  onChange: (v: string) => void
  nodes: GraphNode[]
  types: ObjectType[]
  placeholder?: string
  autoFocus?: boolean
  minRows?: number
  /** Ctrl/Cmd+Enter */
  onDone?: () => void
}

type Wrap = { before: string; after?: string; block?: boolean; placeholder?: string }

const ACTIONS: { icon: typeof Bold; title: string; wrap: Wrap }[] = [
  { icon: Bold, title: 'Жирный (Ctrl+B)', wrap: { before: '**', after: '**', placeholder: 'текст' } },
  { icon: Italic, title: 'Курсив (Ctrl+I)', wrap: { before: '_', after: '_', placeholder: 'текст' } },
  { icon: Heading2, title: 'Заголовок', wrap: { before: '## ', block: true } },
  { icon: List, title: 'Список', wrap: { before: '- ', block: true } },
  { icon: ListOrdered, title: 'Нумерованный', wrap: { before: '1. ', block: true } },
  { icon: CheckSquare, title: 'Чеклист', wrap: { before: '- [ ] ', block: true } },
  { icon: Quote, title: 'Цитата', wrap: { before: '> ', block: true } },
  { icon: Code, title: 'Код', wrap: { before: '`', after: '`', placeholder: 'код' } },
  { icon: LinkIcon, title: 'Ссылка (Ctrl+K)', wrap: { before: '[', after: '](https://)', placeholder: 'текст' } },
  { icon: Brackets, title: 'Вики-ссылка на объект [[…]]', wrap: { before: '[[', after: ']]', placeholder: 'название' } },
]

/** Plain-textarea markdown editor: toolbar, list continuation, `[[` autocomplete over the library. */
export function MdEditor({ value, onChange, nodes, types, placeholder, autoFocus, minRows = 8, onDone }: Props) {
  const ta = useRef<HTMLTextAreaElement>(null)
  const [wiki, setWiki] = useState<{ start: number; query: string } | null>(null)
  const [hover, setHover] = useState(0)

  const apply = useCallback(
    (w: Wrap) => {
      const el = ta.current
      if (!el) return
      const { selectionStart: s, selectionEnd: e } = el
      let next: string
      let cs: number
      let ce: number
      if (w.block) {
        const ls = value.lastIndexOf('\n', s - 1) + 1
        const le = value.indexOf('\n', e) === -1 ? value.length : value.indexOf('\n', e)
        const lines = value.slice(ls, le).split('\n')
        const all = lines.every((l) => l.startsWith(w.before))
        const out = lines.map((l) => (all ? l.slice(w.before.length) : w.before + l)).join('\n')
        next = value.slice(0, ls) + out + value.slice(le)
        cs = ls
        ce = ls + out.length
      } else {
        const sel = value.slice(s, e) || w.placeholder || ''
        next = value.slice(0, s) + w.before + sel + (w.after ?? '') + value.slice(e)
        cs = s + w.before.length
        ce = cs + sel.length
      }
      onChange(next)
      requestAnimationFrame(() => {
        el.focus()
        el.setSelectionRange(cs, ce)
      })
    },
    [value, onChange],
  )

  const candidates = useMemo(() => {
    if (!wiki) return []
    const q = wiki.query.toLowerCase()
    return nodes
      .filter((n) => n.archived === 0 && (!q || n.label.toLowerCase().includes(q) || n.id.includes(q)))
      .sort((a, b) => Number(b.label.toLowerCase().startsWith(q)) - Number(a.label.toLowerCase().startsWith(q)))
      .slice(0, 8)
  }, [wiki, nodes])

  useEffect(() => setHover(0), [wiki?.query])

  const pick = (n: GraphNode) => {
    if (!wiki || !ta.current) return
    const el = ta.current
    const end = el.selectionStart
    const next = value.slice(0, wiki.start) + `[[${n.label}]]` + value.slice(end)
    onChange(next)
    const pos = wiki.start + n.label.length + 4
    setWiki(null)
    requestAnimationFrame(() => {
      el.focus()
      el.setSelectionRange(pos, pos)
    })
  }

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    const el = e.currentTarget
    if (wiki && candidates.length) {
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        setHover((h) => (h + 1) % candidates.length)
        return
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault()
        setHover((h) => (h - 1 + candidates.length) % candidates.length)
        return
      }
      if (e.key === 'Enter' || e.key === 'Tab') {
        e.preventDefault()
        pick(candidates[hover])
        return
      }
      if (e.key === 'Escape') {
        setWiki(null)
        return
      }
    }
    const mod = e.ctrlKey || e.metaKey
    if (mod && e.key === 'Enter') {
      e.preventDefault()
      onDone?.()
      return
    }
    if (mod && e.key.toLowerCase() === 'b') {
      e.preventDefault()
      apply(ACTIONS[0].wrap)
      return
    }
    if (mod && e.key.toLowerCase() === 'i') {
      e.preventDefault()
      apply(ACTIONS[1].wrap)
      return
    }
    if (mod && e.key.toLowerCase() === 'k') {
      e.preventDefault()
      apply(ACTIONS[8].wrap)
      return
    }
    if (e.key === 'Tab') {
      e.preventDefault()
      const s = el.selectionStart
      onChange(value.slice(0, s) + '  ' + value.slice(el.selectionEnd))
      requestAnimationFrame(() => el.setSelectionRange(s + 2, s + 2))
      return
    }
    if (e.key === 'Enter' && !e.shiftKey) {
      // continue lists / checklists / quotes; empty item ends the list
      const s = el.selectionStart
      const ls = value.lastIndexOf('\n', s - 1) + 1
      const line = value.slice(ls, s)
      const m = line.match(/^(\s*)([-*+]|\d+[.)])\s(\[[ xX]\]\s)?/)
      if (m) {
        e.preventDefault()
        const content = line.slice(m[0].length)
        if (!content.trim()) {
          onChange(value.slice(0, ls) + value.slice(s))
          requestAnimationFrame(() => el.setSelectionRange(ls, ls))
          return
        }
        const bullet = /^\d+/.test(m[2]) ? `${parseInt(m[2], 10) + 1}${m[2].endsWith(')') ? ')' : '.'}` : m[2]
        const prefix = `\n${m[1]}${bullet} ${m[3] ? '[ ] ' : ''}`
        onChange(value.slice(0, s) + prefix + value.slice(el.selectionEnd))
        requestAnimationFrame(() => el.setSelectionRange(s + prefix.length, s + prefix.length))
        return
      }
      const q = line.match(/^(\s*>\s)/)
      if (q && line.length > q[0].length) {
        e.preventDefault()
        const prefix = `\n${q[1]}`
        onChange(value.slice(0, s) + prefix + value.slice(el.selectionEnd))
        requestAnimationFrame(() => el.setSelectionRange(s + prefix.length, s + prefix.length))
      }
    }
  }

  const onInput = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const v = e.target.value
    onChange(v)
    const pos = e.target.selectionStart
    const before = v.slice(0, pos)
    const open = before.lastIndexOf('[[')
    if (open !== -1 && !before.slice(open).includes(']]') && !before.slice(open).includes('\n')) {
      setWiki({ start: open, query: before.slice(open + 2) })
    } else if (wiki) {
      setWiki(null)
    }
  }

  const rows = Math.max(minRows, Math.min(40, value.split('\n').length + 2))

  return (
    <div className="relative">
      <div className="mb-1 flex flex-wrap items-center gap-0.5 rounded border border-line bg-panel/60 px-1 py-0.5">
        {ACTIONS.map((a) => (
          <button key={a.title} type="button" onMouseDown={(e) => e.preventDefault()} onClick={() => apply(a.wrap)} className="rounded p-1 text-zinc-500 hover:bg-zinc-800 hover:text-zinc-100" title={a.title}>
            <a.icon size={13} />
          </button>
        ))}
        <span className="ml-auto pr-1 font-mono text-[9px] text-zinc-600">markdown · [[объект]] · Ctrl+Enter — готово</span>
      </div>
      <textarea
        ref={ta}
        value={value}
        autoFocus={autoFocus}
        onChange={onInput}
        onKeyDown={onKeyDown}
        onBlur={() => setTimeout(() => setWiki(null), 150)}
        rows={rows}
        placeholder={placeholder ?? 'Пиши в markdown. [[ — ссылка на объект библиотеки, - [ ] — чеклист…'}
        spellCheck
        className="w-full resize-y rounded border border-line bg-panel px-2.5 py-2 font-mono text-[12px] leading-relaxed text-zinc-200 placeholder:text-zinc-600 focus:border-acid focus:outline-none"
      />
      {wiki && candidates.length > 0 && (
        <div className="absolute left-2 right-2 z-20 mt-[-6px] max-h-56 overflow-y-auto rounded border border-line bg-void shadow-2xl">
          {candidates.map((n, i) => {
            const t = types.find((x) => x.id === n.type_id)
            return (
              <button
                key={n.id}
                type="button"
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => pick(n)}
                className={`flex w-full items-center gap-2 px-2 py-1 text-left text-[12px] ${i === hover ? 'bg-zinc-800 text-zinc-100' : 'text-zinc-300 hover:bg-zinc-900'}`}
              >
                <span style={{ color: t?.color }}>{t?.icon ?? '◇'}</span>
                <span className="truncate">{n.label}</span>
                <span className="ml-auto font-mono text-[9px] text-zinc-600">{t?.name}</span>
              </button>
            )
          })}
        </div>
      )}
    </div>
  )
}
