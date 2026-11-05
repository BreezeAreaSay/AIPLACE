import { useMemo } from 'react'
import ReactMarkdown, { defaultUrlTransform, type Components } from 'react-markdown'
import remarkGfm from 'remark-gfm'
import remarkBreaks from 'remark-breaks'
import rehypeHighlight from 'rehype-highlight'
import type { GraphNode, ObjectType } from '../types'

interface Props {
  text: string
  nodes?: GraphNode[]
  types?: ObjectType[]
  /** Open an object from a `[[wikilink]]` or an object chip. */
  onOpen?: (id: string) => void
  /** Create a note for an unresolved `[[wikilink]]`. */
  onCreate?: (title: string) => void
  /** GFM task checkboxes become interactive: receives the updated source. */
  onChange?: (next: string) => void
  /** `note` — full note styling; `chat` — compact bubble styling. */
  variant?: 'note' | 'chat'
  breaks?: boolean
}

const WIKI = /\[\[([^\]|#]+)(?:#[^\]|]*)?(?:\|([^\]]+))?\]\]/g

/** Turn `[[Target|alias]]` into a markdown link with a `wiki:` scheme so the renderer can resolve it. */
export function preprocessWikilinks(text: string): string {
  return text.replace(WIKI, (_m, target: string, alias?: string) => `[${(alias ?? target).trim()}](wiki:${encodeURIComponent(target.trim())})`)
}

export function resolveWikilink(target: string, nodes: GraphNode[]): GraphNode | undefined {
  const t = target.trim().toLowerCase()
  return nodes.find((n) => n.id === t) ?? nodes.find((n) => n.label.toLowerCase() === t) ?? nodes.find((n) => n.label.toLowerCase().startsWith(t))
}

/** Toggle the n-th task checkbox (`- [ ]` ↔ `- [x]`) in markdown source. */
export function toggleTask(source: string, index: number): string {
  let seen = -1
  return source
    .split('\n')
    .map((line) => {
      const m = line.match(/^(\s*(?:[-*+]|\d+[.)])\s+)\[( |x|X)\](\s|$)/)
      if (!m) return line
      seen += 1
      if (seen !== index) return line
      const open = m[2] === ' '
      return line.replace(/\[( |x|X)\]/, open ? '[x]' : '[ ]')
    })
    .join('\n')
}

/** Shared markdown renderer: GFM (tables, task lists, strikethrough), wikilinks, safe external links. */
export function Markdown({ text, nodes = [], types = [], onOpen, onCreate, onChange, variant = 'note', breaks = variant === 'chat' }: Props) {
  // HTML comments (digest markers etc.) are bookkeeping, never content.
  const src = useMemo(() => preprocessWikilinks(text.replace(/<!--[\s\S]*?-->/g, '')), [text])
  let taskIndex = -1

  const components: Components = {
    a: ({ href, children }) => {
      if (href?.startsWith('wiki:')) {
        const target = decodeURIComponent(href.slice(5))
        const n = resolveWikilink(target, nodes)
        if (n) {
          const t = types.find((x) => x.id === n.type_id)
          return (
            <button type="button" onClick={() => onOpen?.(n.id)} className="wiki inline-flex max-w-full items-baseline gap-0.5 align-baseline" title={`${t?.name ?? 'объект'}: открыть`}>
              <span style={{ color: t?.color }}>{t?.icon ?? '◇'}</span>
              <span className="truncate">{children}</span>
            </button>
          )
        }
        return (
          <button type="button" onClick={() => onCreate?.(target)} className="wiki wiki-missing align-baseline" title="Объекта нет — создать заметку">
            {children}
          </button>
        )
      }
      const external = /^https?:/i.test(href ?? '')
      return (
        <a href={href} target={external ? '_blank' : undefined} rel={external ? 'noreferrer noopener' : undefined}>
          {children}
        </a>
      )
    },
    input: ({ checked, type }) => {
      if (type !== 'checkbox') return null
      taskIndex += 1
      const i = taskIndex
      return (
        <input
          type="checkbox"
          checked={!!checked}
          disabled={!onChange}
          onChange={() => onChange?.(toggleTask(text, i))}
          className="mr-1.5 h-3.5 w-3.5 cursor-pointer accent-[#CCFF00] align-[-2px] disabled:cursor-default"
        />
      )
    },
    pre: ({ children }) => <pre className="md-pre">{children}</pre>,
    blockquote: ({ children }) => {
      // Obsidian-style callouts: > [!note] / [!tip] / [!warning] / [!quote]
      const arr = Array.isArray(children) ? children : [children]
      const first = arr.find((c) => c && typeof c === 'object' && 'props' in (c as object)) as { props?: { children?: unknown } } | undefined
      const text = (() => {
        const ch = first?.props?.children
        if (typeof ch === 'string') return ch
        if (Array.isArray(ch) && typeof ch[0] === 'string') return ch[0] as string
        return ''
      })()
      const m = text.match(/^\[!(\w+)\]\s*/)
      if (m) {
        const kind = m[1].toLowerCase()
        return <blockquote className={`md-callout md-callout-${kind}`} data-kind={kind}>{children}</blockquote>
      }
      return <blockquote>{children}</blockquote>
    },
    table: ({ children }) => (
      <div className="md-table-wrap">
        <table>{children}</table>
      </div>
    ),
  }

  return (
    <div className={variant === 'chat' ? 'chat-md' : 'note-md'}>
      <ReactMarkdown remarkPlugins={breaks ? [remarkGfm, remarkBreaks] : [remarkGfm]} rehypePlugins={variant === 'note' ? [[rehypeHighlight, { detect: false, ignoreMissing: true }]] : []} components={components} urlTransform={(u) => (u.startsWith('wiki:') ? u : defaultUrlTransform(u))}>
        {src}
      </ReactMarkdown>
    </div>
  )
}
