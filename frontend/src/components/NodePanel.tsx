import { useEffect, useState } from 'react'
import ReactMarkdown from 'react-markdown'
import { Archive, Eye, Link2, MessageSquare, Pencil, Pin, PinOff, Send, Swords, Trash2, TrendingUp, X } from 'lucide-react'
import type { Agent, Graph, GraphNode, Message, NodeNote, ObjectType } from '../types'
import { AGENT_COLORS, RELATION_KIND_LABEL } from '../types'
import { PropsEditor } from './PropsEditor'
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
}

export function NodePanel({ node, graph, types, notes, agents, disabled, onClose, onPatch, onDelete, onAsk, onAddNote, onDeleteEdge, onSelect }: Props) {
  const [label, setLabel] = useState(node.label)
  const [body, setBody] = useState(node.body)
  const [editing, setEditing] = useState(false)
  const [myNote, setMyNote] = useState('')
  const [question, setQuestion] = useState('')
  const [backlinks, setBacklinks] = useState<Message[]>([])

  useEffect(() => {
    setLabel(node.label)
    setBody(node.body)
    setEditing(false)
  }, [node.id, node.label, node.body])

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

  const name = (id: string) => (id === 'user' ? 'ты' : id === 'archivist' ? 'архивариус' : agents.find((a) => a.id === id)?.name.split(' ')[0] ?? id)
  const mine = notes.filter((n) => n.node_id === node.id)
  const links = graph.edges.filter((e) => e.source_id === node.id || e.target_id === node.id)
  const type = types.find((t) => t.id === node.type_id)

  const commit = () => {
    const patch: { label?: string; body?: string } = {}
    if (label.trim() && label.trim() !== node.label) patch.label = label.trim()
    if (body !== node.body) patch.body = body
    if (Object.keys(patch).length) onPatch(node.id, patch)
    setEditing(false)
  }

  return (
    <div className="flex h-full w-full flex-col bg-void">
      <div className="flex items-center gap-2 border-b border-line px-3 py-2">
        <span className="text-base" style={{ color: type?.color }} title={type?.name}>
          {type?.icon ?? '◇'}
        </span>
        <select
          value={node.type_id}
          onChange={(e) => onPatch(node.id, { type_id: e.target.value })}
          className="rounded border border-line bg-panel px-1 py-0.5 font-mono text-[10px] text-zinc-300"
          title="Тип объекта"
        >
          {types.map((t) => (
            <option key={t.id} value={t.id}>
              {t.icon} {t.name}
            </option>
          ))}
        </select>
        <span className="font-mono text-[10px] text-zinc-500">· {name(node.created_by)}</span>
        <div className="ml-auto flex items-center gap-1">
          <button onClick={() => onPatch(node.id, { pinned: node.pinned === 0 })} className="rounded p-1 text-zinc-500 hover:text-zinc-200" title={node.pinned ? 'Открепить' : 'Закрепить (не двигается)'}>
            {node.pinned ? <PinOff size={14} /> : <Pin size={14} />}
          </button>
          <button onClick={() => setEditing((v) => !v)} className={`rounded p-1 hover:text-zinc-200 ${editing ? 'text-acid' : 'text-zinc-500'}`} title="Редактировать">
            {editing ? <Eye size={14} /> : <Pencil size={14} />}
          </button>
          <button onClick={() => onPatch(node.id, { archived: node.archived === 0 })} className={`rounded p-1 hover:text-zinc-200 ${node.archived ? 'text-acid' : 'text-zinc-500'}`} title={node.archived ? 'Вернуть из архива' : 'В архив (скрыть из библиотеки)'}>
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
          <button onClick={onClose} className="rounded p-1 text-zinc-500 hover:text-zinc-200" aria-label="Закрыть">
            <X size={16} />
          </button>
        </div>
      </div>

      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-3 py-3">
        {editing ? (
          <div className="space-y-2">
            <input
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              className="w-full rounded border border-line bg-panel px-2 py-1.5 text-sm font-semibold text-zinc-100 focus:border-acid focus:outline-none"
            />
            <textarea
              value={body}
              onChange={(e) => setBody(e.target.value)}
              rows={10}
              placeholder="Тело заметки (markdown)…"
              className="w-full resize-y rounded border border-line bg-panel px-2 py-1.5 font-mono text-xs leading-relaxed text-zinc-200 placeholder:text-zinc-600 focus:border-acid focus:outline-none"
            />
            <div className="flex gap-2">
              <button onClick={commit} className="rounded bg-acid px-3 py-1 font-mono text-xs font-bold text-void">
                сохранить
              </button>
              <button onClick={() => setEditing(false)} className="rounded border border-line px-3 py-1 font-mono text-xs text-zinc-400">
                отмена
              </button>
            </div>
          </div>
        ) : (
          <div>
            <h2 className="text-base font-semibold leading-snug text-zinc-100">{node.label}</h2>
            <div className="mt-1 font-mono text-[10px] text-zinc-600">
              {node.id} · {new Date(node.created_at).toLocaleString('ru-RU', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' })}
            </div>
            {node.body.trim() ? (
              <div className="note-md mt-2 text-[13px] leading-relaxed text-zinc-300">
                <ReactMarkdown>{node.body}</ReactMarkdown>
              </div>
            ) : (
              <button onClick={() => setEditing(true)} className="mt-2 text-xs text-zinc-600 underline hover:text-zinc-400">
                тело пустое — написать
              </button>
            )}
          </div>
        )}

        {type && type.schema.props.length > 0 && (
          <section>
            <div className="mb-1.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">свойства · {type.name.toLowerCase()}</div>
            <PropsEditor schema={type.schema.props} values={node.props} nodes={graph.nodes} onChange={(k, v) => onPatch(node.id, { props: { [k]: v } })} />
          </section>
        )}

        <section>
          <div className="mb-1.5 flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">
            <MessageSquare size={11} /> позвать триаду
          </div>
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
              placeholder="спросить про этот узел…"
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
            {mine.length === 0 && <div className="text-[11px] text-zinc-600">Пока никто ничего не написал на этом узле.</div>}
            {mine.map((n) => (
              <div key={n.id} className="rounded border-l-2 bg-panel px-2.5 py-1.5 text-[12px] leading-snug text-zinc-200" style={{ borderColor: AGENT_COLORS[n.author] ?? '#666' }}>
                <span className="mr-1.5 font-mono text-[10px]" style={{ color: AGENT_COLORS[n.author] }}>
                  {name(n.author)}
                </span>
                {n.text}
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
          {links.length === 0 && <div className="text-[11px] text-zinc-600">Потяни за точку на карточке к другому узлу, чтобы связать.</div>}
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
