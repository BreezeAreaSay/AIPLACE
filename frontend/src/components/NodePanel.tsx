import { useCallback, useEffect, useRef, useState } from 'react'
import { Archive, BookOpen, Check, Copy, Download, Eye, Files, Github, Leaf, Link2, MessageSquare, Pencil, Pin, PinOff, Send, Sparkles, Swords, Trash2, TrendingUp, X } from 'lucide-react'
import type { Agent, Attachment, Features, Graph, GraphNode, Message, NodeNote, ObjectType } from '../types'
import { AGENT_COLORS, RELATION_KIND_LABEL } from '../types'
import { PropsEditor } from './PropsEditor'
import { Markdown } from './Markdown'
import { MdEditor } from './MdEditor'
import { Attachments } from './Attachments'
import { NoteScope } from '../live/LiveContext'
import { api } from '../api'

interface Props {
  node: GraphNode
  graph: Graph
  types: ObjectType[]
  notes: NodeNote[]
  agents: Agent[]
  disabled: boolean
  onClose: () => void
  onPatch: (id: string, patch: { label?: string; body?: string; type?: string; type_id?: string; props?: Record<string, unknown>; archived?: boolean; pinned?: boolean }) => void
  onDelete: (id: string) => void
  onAsk: (id: string, mode: 'discuss' | 'develop' | 'dispute', message?: string) => void
  onAddNote: (id: string, text: string) => void
  onDeleteEdge: (id: number) => void
  onSelect: (id: string) => void
  onCreate: (label: string, opts: { type_id?: string; body?: string; props?: Record<string, unknown> }) => void
  attachments: Attachment[]
  features: Features
  onUpload: (id: string, files: File[]) => Promise<unknown>
  onDeleteFile: (id: string) => void
  onAutofill: (id: string) => Promise<{ found: boolean; filled?: string[]; card?: { source: string } }>
  onGithub: (id: string) => Promise<unknown>
  onTend: (nodeId?: string) => Promise<unknown>
}

function wordCount(s: string) {
  return s.trim() ? s.trim().split(/\s+/).length : 0
}

/** Object page: editable markdown note with autosave, properties, triad buttons, notes, links, backlinks. */
export function NodePanel({ node, graph, types, notes, agents, disabled, onClose, onPatch, onDelete, onAsk, onAddNote, onDeleteEdge, onSelect, onCreate, attachments, features, onUpload, onDeleteFile, onAutofill, onGithub, onTend }: Props) {
  const [label, setLabel] = useState(node.label)
  const [body, setBody] = useState(node.body)
  const [editing, setEditing] = useState(() => !node.body.trim())
  const [saved, setSaved] = useState<'idle' | 'dirty' | 'saved'>('idle')
  const [copied, setCopied] = useState(false)
  const [reading, setReading] = useState(false)
  const [sel, setSel] = useState<{ text: string; x: number; y: number } | null>(null)
  const [myNote, setMyNote] = useState('')
  const [question, setQuestion] = useState('')
  const [backlinks, setBacklinks] = useState<Message[]>([])
  const [filling, setFilling] = useState<string | null>(null)
  const timer = useRef<number | null>(null)
  const lastSent = useRef({ label: node.label, body: node.body })

  // New object selected: reset local draft (but never clobber a draft of the same object).
  useEffect(() => {
    setLabel(node.label)
    setBody(node.body)
    lastSent.current = { label: node.label, body: node.body }
    setEditing(!node.body.trim())
    setSaved('idle')
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [node.id])

  // Remote change (agent appended, import) while not editing → follow it.
  useEffect(() => {
    if (!editing && node.body !== lastSent.current.body) {
      setBody(node.body)
      lastSent.current.body = node.body
    }
    if (!editing && node.label !== lastSent.current.label) {
      setLabel(node.label)
      lastSent.current.label = node.label
    }
  }, [node.body, node.label, editing])

  useEffect(() => {
    if (!reading) return
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && setReading(false)
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [reading])

  useEffect(() => {
    let alive = true
    api
      .backlinks(node.id)
      .then((b) => alive && setBacklinks(b))
      .catch(() => alive && setBacklinks([]))
    return () => {
      alive = false
    }
  }, [node.id, node.updated_at])

  const flush = useCallback(
    (l: string, b: string) => {
      const patch: { label?: string; body?: string } = {}
      const title = l.trim()
      if (title && title !== lastSent.current.label) patch.label = title
      if (b !== lastSent.current.body) patch.body = b
      if (Object.keys(patch).length) {
        onPatch(node.id, patch)
        lastSent.current = { label: patch.label ?? lastSent.current.label, body: patch.body ?? lastSent.current.body }
        setSaved('saved')
        window.setTimeout(() => setSaved((s) => (s === 'saved' ? 'idle' : s)), 1500)
      } else {
        setSaved('idle')
      }
    },
    [node.id, onPatch],
  )

  // Autosave: 900ms after the last keystroke, and on unmount / object switch.
  const schedule = (l: string, b: string) => {
    setSaved('dirty')
    if (timer.current) window.clearTimeout(timer.current)
    timer.current = window.setTimeout(() => flush(l, b), 900)
  }
  const draft = useRef({ label, body })
  draft.current = { label, body }
  useEffect(
    () => () => {
      if (timer.current) {
        window.clearTimeout(timer.current)
        flush(draft.current.label, draft.current.body)
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [node.id],
  )

  const changeBody = (v: string) => {
    setBody(v)
    schedule(label, v)
  }
  const changeLabel = (v: string) => {
    setLabel(v)
    schedule(v, body)
  }
  const finish = () => {
    if (timer.current) window.clearTimeout(timer.current)
    flush(label, body)
    setEditing(false)
  }

  const name = (id: string) => (id === 'user' ? 'ты' : id === 'archivist' ? 'архивариус' : id === 'mcp' ? 'mcp' : agents.find((a) => a.id === id)?.name.split(' ')[0] ?? id)
  const mine = notes.filter((n) => n.node_id === node.id)
  const links = graph.edges.filter((e) => e.source_id === node.id || e.target_id === node.id)
  const type = types.find((t) => t.id === node.type_id)
  const lookupKind = node.type_id === 'book' ? 'book' : node.type_id === 'movie' ? 'movie' : ['human', 'place', 'idea', 'note', 'page', 'quote', 'event', 'project'].includes(node.type_id) ? 'wiki' : null

  const asMarkdown = () => {
    const fm = [`title: ${node.label}`, `type: ${node.type_id}`, ...Object.entries(node.props).filter(([, v]) => v !== null && v !== '').map(([k, v]) => `${k}: ${typeof v === 'string' ? v : JSON.stringify(v)}`)]
    return `---\n${fm.join('\n')}\n---\n\n# ${node.label}\n\n${body.trim()}\n`
  }
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(asMarkdown())
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1200)
    } catch {
      /* clipboard unavailable */
    }
  }

  const iconBtn = 'rounded p-1 text-zinc-500 hover:text-zinc-200'

  return (
    <div className="flex h-full w-full flex-col bg-void">
      {reading && (
        <div className="fixed inset-0 z-[60] overflow-y-auto bg-void/98 px-4 py-8" onClick={() => setReading(false)}>
          <div className="mx-auto max-w-[76ch]" onClick={(e) => e.stopPropagation()}>
            <div className="mb-6 flex items-start gap-3">
              <span className="text-2xl" style={{ color: type?.color }}>
                {type?.icon ?? '◇'}
              </span>
              <div className="min-w-0 flex-1">
                <h1 className="text-2xl font-semibold leading-tight text-zinc-50">{node.label}</h1>
                {typeof node.props.summary === 'string' && node.props.summary.trim() && <div className="mt-1 text-sm italic text-zinc-400">{node.props.summary}</div>}
                <div className="mt-1 font-mono text-[10px] text-zinc-600">
                  {type?.name} · {name(node.created_by)} · {wordCount(body)} слов · Esc — закрыть
                </div>
              </div>
              <button onClick={() => setReading(false)} className="rounded p-1 text-zinc-500 hover:text-zinc-200" aria-label="Закрыть">
                <X size={18} />
              </button>
            </div>
            <div className="text-[15px]">
              <NoteScope nodeId={node.id}>
                <Markdown text={body} nodes={graph.nodes} types={types} onOpen={(id) => { setReading(false); onSelect(id) }} onChange={changeBody} />
              </NoteScope>
            </div>
          </div>
        </div>
      )}
      <div className="flex items-center gap-2 border-b border-line px-3 py-2">
        <span className="text-base" style={{ color: type?.color }} title={type?.name}>
          {type?.icon ?? '◇'}
        </span>
        <select value={node.type_id} onChange={(e) => onPatch(node.id, { type_id: e.target.value })} className="rounded border border-line bg-panel px-1 py-0.5 font-mono text-[10px] text-zinc-300" title="Тип объекта">
          {types.map((t) => (
            <option key={t.id} value={t.id}>
              {t.icon} {t.name}
            </option>
          ))}
        </select>
        <span className="font-mono text-[10px] text-zinc-500">· {name(node.created_by)}</span>
        <span className={`font-mono text-[9px] ${saved === 'dirty' ? 'text-zinc-500' : saved === 'saved' ? 'text-acid' : 'text-transparent'}`}>{saved === 'dirty' ? '…' : '✓ сохранено'}</span>
        <div className="ml-auto flex items-center gap-0.5">
          <button onClick={() => onPatch(node.id, { pinned: node.pinned === 0 })} className={iconBtn} title={node.pinned ? 'Открепить' : 'Закрепить'}>
            {node.pinned ? <PinOff size={14} /> : <Pin size={14} />}
          </button>
          <button onClick={() => (editing ? finish() : setEditing(true))} className={`rounded p-1 hover:text-zinc-200 ${editing ? 'text-acid' : 'text-zinc-500'}`} title={editing ? 'Готово — показать (Ctrl+Enter)' : 'Редактировать'}>
            {editing ? <Eye size={14} /> : <Pencil size={14} />}
          </button>
          <button onClick={() => setReading(true)} className={iconBtn} title="Режим чтения">
            <BookOpen size={14} />
          </button>
          <button onClick={() => void copy()} className={iconBtn} title="Скопировать как markdown">
            {copied ? <Check size={14} className="text-acid" /> : <Copy size={14} />}
          </button>
          <a href={api.nodeExportUrl(node.id)} download className={iconBtn} title="Скачать .md (с frontmatter и связями)">
            <Download size={14} />
          </a>
          <button onClick={() => onCreate(`${node.label} (копия)`, { type_id: node.type_id, body, props: node.props })} className={iconBtn} title="Дублировать">
            <Files size={14} />
          </button>
          <button onClick={() => onPatch(node.id, { archived: node.archived === 0 })} className={`rounded p-1 hover:text-zinc-200 ${node.archived ? 'text-acid' : 'text-zinc-500'}`} title={node.archived ? 'Вернуть из архива' : 'В архив'}>
            <Archive size={14} />
          </button>
          <button
            onClick={() => {
              if (confirm(`Удалить «${node.label}» со всеми пометками и связями?`)) {
                onDelete(node.id)
                onClose()
              }
            }}
            className="rounded p-1 text-zinc-500 hover:text-magenta"
            title="Удалить"
          >
            <Trash2 size={14} />
          </button>
          <button onClick={onClose} className={iconBtn} aria-label="Закрыть">
            <X size={16} />
          </button>
        </div>
      </div>

      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-3 py-3">
        <div>
          <input
            value={label}
            onChange={(e) => changeLabel(e.target.value)}
            onBlur={() => flush(label, body)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault()
                ;(e.target as HTMLInputElement).blur()
              }
            }}
            className="w-full bg-transparent text-base font-semibold leading-snug text-zinc-100 focus:outline-none"
            placeholder="Заголовок"
            aria-label="Заголовок"
          />
          {typeof node.props.summary === 'string' && node.props.summary.trim() && !editing && <div className="mt-0.5 text-[12px] italic text-zinc-400">{node.props.summary}</div>}
          <div className="mt-0.5 flex items-center gap-2 font-mono text-[10px] text-zinc-600">
            <span>{node.id}</span>
            <span>·</span>
            <span>{new Date(node.updated_at).toLocaleString('ru-RU', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' })}</span>
            <span>·</span>
            <span>{wordCount(body)} слов</span>
          </div>
          {editing ? (
            <div className="mt-2">
              <MdEditor value={body} onChange={changeBody} nodes={graph.nodes.filter((n) => n.id !== node.id)} types={types} autoFocus onDone={finish} />
            </div>
          ) : (
            <div
              className="relative mt-2 cursor-text text-[13px] leading-relaxed text-zinc-300"
              onDoubleClick={() => setEditing(true)}
              title="Двойной клик — редактировать · выдели текст, чтобы обсудить фрагмент"
              onMouseUp={(e) => {
                const s = window.getSelection()?.toString().trim() ?? ''
                if (s.length >= 8 && s.length <= 600) {
                  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
                  setSel({ text: s, x: Math.min(e.clientX - rect.left, rect.width - 160), y: e.clientY - rect.top })
                } else setSel(null)
              }}
            >
              {sel && (
                <div className="absolute z-20 flex gap-1 rounded border border-line bg-void p-1 shadow-xl" style={{ left: Math.max(0, sel.x), top: sel.y + 8 }}>
                  <button
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => {
                      onAsk(node.id, 'discuss', `Про этот фрагмент: «${sel.text.slice(0, 300)}» — что скажете?`)
                      setSel(null)
                    }}
                    className="rounded bg-acid px-2 py-0.5 font-mono text-[10px] font-bold text-void"
                  >
                    обсудить с двором
                  </button>
                  <button
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => {
                      onAsk(node.id, 'dispute', `Оспорьте этот фрагмент: «${sel.text.slice(0, 300)}»`)
                      setSel(null)
                    }}
                    className="rounded border border-magenta/50 px-2 py-0.5 font-mono text-[10px] text-magenta"
                  >
                    оспорить
                  </button>
                  <button onMouseDown={(e) => e.preventDefault()} onClick={() => setSel(null)} className="px-1 font-mono text-[10px] text-zinc-500">
                    ×
                  </button>
                </div>
              )}
              {body.trim() ? (
                <NoteScope nodeId={node.id}>
                  <Markdown text={body} nodes={graph.nodes} types={types} onOpen={onSelect} onCreate={(title) => onCreate(title, { type_id: 'note' })} onChange={changeBody} />
                </NoteScope>
              ) : (
                <button onClick={() => setEditing(true)} className="text-xs text-zinc-600 underline hover:text-zinc-400">
                  тело пустое — написать
                </button>
              )}
            </div>
          )}
        </div>

        {type && (type.schema.props.length > 0 || lookupKind) && (
          <section>
            <div className="mb-1.5 flex items-center gap-2 font-mono text-[10px] uppercase tracking-widest text-zinc-500">
              свойства · {type.name.toLowerCase()}
              {lookupKind && (lookupKind !== 'movie' || features.tmdb) && (
                <button
                  onClick={() => {
                    setFilling('…')
                    void onAutofill(node.id)
                      .then((r) => setFilling(r.found ? `заполнено: ${(r.filled ?? []).join(', ') || 'описание'} (${r.card?.source ?? ''})` : 'ничего не нашлось'))
                      .catch((e) => setFilling(`ошибка: ${(e as Error).message}`))
                      .finally(() => window.setTimeout(() => setFilling(null), 4000))
                  }}
                  disabled={!!filling}
                  className="ml-auto flex items-center gap-1 rounded border border-acid/50 px-1.5 py-0.5 text-[10px] normal-case tracking-normal text-acid hover:bg-acid/10 disabled:opacity-50"
                  title={lookupKind === 'book' ? 'OpenLibrary: автор, год, страницы, обложка' : lookupKind === 'movie' ? 'TMDB: год, режиссёр, постер, описание' : 'Википедия: описание и ссылка'}
                >
                  <Sparkles size={10} /> {filling ?? 'подтянуть данные'}
                </button>
              )}
              {node.type_id === 'project' && typeof node.props.repo === 'string' && node.props.repo && (
                <button
                  onClick={() => {
                    setFilling('GitHub…')
                    void onGithub(node.id)
                      .then(() => setFilling('активность дописана в заметку'))
                      .catch((e) => setFilling(`ошибка: ${(e as Error).message}`))
                      .finally(() => window.setTimeout(() => setFilling(null), 4000))
                  }}
                  disabled={!!filling}
                  className="ml-auto flex items-center gap-1 rounded border border-line px-1.5 py-0.5 text-[10px] normal-case tracking-normal text-zinc-300 hover:border-acid hover:text-acid disabled:opacity-50"
                  title="Коммиты и issues — в раздел «GitHub» заметки"
                >
                  <Github size={10} /> {filling ?? 'GitHub'}
                </button>
              )}
            </div>
            <NoteScope nodeId={node.id}>
              <PropsEditor schema={type.schema.props} values={node.props} nodes={graph.nodes} onChange={(k, v) => onPatch(node.id, { props: { [k]: v } })} onOpen={onSelect} />
            </NoteScope>
          </section>
        )}

        <Attachments nodeId={node.id} attachments={attachments} digested={new Set(attachments.filter((a) => node.body.includes(`<!-- digest:${a.id} -->`)).map((a) => a.id))} onUpload={(files) => onUpload(node.id, files)} onDelete={onDeleteFile} />

        <section>
          <div className="mb-1.5 flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">
            <MessageSquare size={11} /> позвать триаду
          </div>
          <button
            onClick={() => {
              setFilling('разбирают…')
              void onTend(node.id)
                .then(() => setFilling('разобрано: смотри тело, пометки и чат'))
                .catch((e) => setFilling(`ошибка: ${(e as Error).message}`))
                .finally(() => window.setTimeout(() => setFilling(null), 4000))
            }}
            disabled={disabled || !!filling}
            className="mb-1.5 flex w-full items-center justify-center gap-1.5 rounded border border-dashed border-acid/50 px-2 py-1.5 font-mono text-[11px] text-acid hover:bg-acid/10 disabled:opacity-40"
            title="Подходящий по роли житель прочитает объект и файлы, допишет главное, свяжет, заполнит поля и скажет об этом в чат"
          >
            <Leaf size={11} /> {filling && filling.startsWith('разб') ? filling : 'разобрать: прочитать, дописать, связать'}
          </button>
          <div className="grid grid-cols-3 gap-1.5">
            <button onClick={() => onAsk(node.id, 'discuss')} disabled={disabled} className="rounded border border-cyan/50 px-2 py-1.5 font-mono text-[11px] text-cyan hover:bg-cyan/10 disabled:opacity-40">
              обсудить
            </button>
            <button onClick={() => onAsk(node.id, 'develop')} disabled={disabled} className="flex items-center justify-center gap-1 rounded border border-acid/50 px-2 py-1.5 font-mono text-[11px] text-acid hover:bg-acid/10 disabled:opacity-40">
              <TrendingUp size={11} /> развить
            </button>
            <button onClick={() => onAsk(node.id, 'dispute')} disabled={disabled} className="flex items-center justify-center gap-1 rounded border border-magenta/50 px-2 py-1.5 font-mono text-[11px] text-magenta hover:bg-magenta/10 disabled:opacity-40">
              <Swords size={11} /> оспорить
            </button>
          </div>
          <div className="mt-1.5 flex gap-1.5">
            <input
              value={question}
              onChange={(e) => setQuestion(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && question.trim()) {
                  onAsk(node.id, 'discuss', question.trim())
                  setQuestion('')
                }
              }}
              placeholder="спросить про этот объект… (они могут дописать в него)"
              className="flex-1 rounded border border-line bg-panel px-2 py-1.5 text-xs text-zinc-100 placeholder:text-zinc-600 focus:border-acid focus:outline-none"
            />
            <button
              onClick={() => {
                if (question.trim()) {
                  onAsk(node.id, 'discuss', question.trim())
                  setQuestion('')
                }
              }}
              disabled={disabled || !question.trim()}
              className="rounded bg-acid px-2 text-void disabled:opacity-40"
            >
              <Send size={13} />
            </button>
          </div>
        </section>

        <section>
          <div className="mb-1.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">пометки · {mine.length}</div>
          <div className="space-y-1.5">
            {mine.length === 0 && <div className="text-[11px] text-zinc-600">Пока никто ничего не написал на этом объекте.</div>}
            {mine.map((n) => (
              <div key={n.id} className="rounded border-l-2 bg-panel px-2.5 py-1.5 text-[12px] leading-snug text-zinc-200" style={{ borderColor: AGENT_COLORS[n.author] ?? '#666' }}>
                <span className="mr-1.5 font-mono text-[10px]" style={{ color: AGENT_COLORS[n.author] }}>
                  {name(n.author)}
                </span>
                <Markdown text={n.text} nodes={graph.nodes} types={types} onOpen={onSelect} variant="chat" />
              </div>
            ))}
          </div>
          <div className="mt-1.5 flex gap-1.5">
            <input
              value={myNote}
              onChange={(e) => setMyNote(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && myNote.trim()) {
                  onAddNote(node.id, myNote.trim())
                  setMyNote('')
                }
              }}
              placeholder="своя пометка…"
              className="flex-1 rounded border border-line bg-panel px-2 py-1.5 text-xs text-zinc-100 placeholder:text-zinc-600 focus:border-acid focus:outline-none"
            />
          </div>
        </section>

        <section>
          <div className="mb-1.5 flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">
            <Link2 size={11} /> связи · {links.length}
          </div>
          {links.length === 0 && <div className="text-[11px] text-zinc-600">Напиши в тексте [[Название]] — или потяни за точку на графе к другому объекту.</div>}
          <div className="space-y-1">
            {links.map((e) => {
              const otherId = e.source_id === node.id ? e.target_id : e.source_id
              const other = graph.nodes.find((n) => n.id === otherId)
              return (
                <div key={e.id} className="flex items-center gap-2 text-[11px]">
                  <span className="text-zinc-600">{e.source_id === node.id ? '→' : '←'}</span>
                  <span className="font-mono text-[10px] text-zinc-500">{e.relation_label || RELATION_KIND_LABEL[e.kind] || e.kind}</span>
                  <button onClick={() => onSelect(otherId)} className="truncate text-left text-zinc-300 underline-offset-2 hover:underline">
                    {other?.label ?? otherId}
                  </button>
                  <button onClick={() => onDeleteEdge(e.id)} className="ml-auto text-zinc-600 hover:text-magenta" title="Убрать связь">
                    <X size={12} />
                  </button>
                </div>
              )
            })}
          </div>
        </section>

        <section>
          <div className="mb-1.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">где всплывало · {backlinks.length}</div>
          {backlinks.length === 0 && <div className="text-[11px] text-zinc-600">Архивариус ещё не связал этот объект с репликами.</div>}
          <div className="space-y-1">
            {backlinks.slice(-12).map((m) => (
              <div key={m.id} className="rounded border border-line/60 bg-panel/50 px-2 py-1 text-[11px] text-zinc-400">
                <span className="mr-1.5 font-mono text-[10px]" style={{ color: AGENT_COLORS[m.sender_id] ?? '#888' }}>
                  {m.sender_id === 'user' ? 'ты' : name(m.sender_id)} · {new Date(m.created_at).toLocaleDateString('ru-RU', { day: '2-digit', month: '2-digit' })}
                </span>
                <span className="text-zinc-300">{m.content.length > 160 ? m.content.slice(0, 160) + '…' : m.content}</span>
              </div>
            ))}
          </div>
        </section>
      </div>
    </div>
  )
}
