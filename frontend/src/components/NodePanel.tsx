import { useEffect, useState } from 'react'
import ReactMarkdown from 'react-markdown'
import { Eye, Link2, MessageSquare, Pencil, Pin, PinOff, Send, Swords, Trash2, TrendingUp, X } from 'lucide-react'
import type { Agent, Graph, GraphNode, NodeNote } from '../types'
import { AGENT_COLORS, NODE_TYPE_LABEL } from '../types'

interface Props {
  node: GraphNode
  graph: Graph
  notes: NodeNote[]
  agents: Agent[]
  disabled: boolean
  onClose: () => void
  onPatch: (id: string, patch: { label?: string; body?: string; type?: string; pinned?: boolean }) => void
  onDelete: (id: string) => void
  onAsk: (id: string, mode: 'discuss' | 'develop' | 'dispute', message?: string) => void
  onAddNote: (id: string, text: string) => void
  onDeleteEdge: (id: number) => void
  onSelect: (id: string) => void
}

export function NodePanel({ node, graph, notes, agents, disabled, onClose, onPatch, onDelete, onAsk, onAddNote, onDeleteEdge, onSelect }: Props) {
  const [label, setLabel] = useState(node.label)
  const [body, setBody] = useState(node.body)
  const [editing, setEditing] = useState(false)
  const [myNote, setMyNote] = useState('')
  const [question, setQuestion] = useState('')

  useEffect(() => {
    setLabel(node.label)
    setBody(node.body)
    setEditing(false)
  }, [node.id, node.label, node.body])

  const name = (id: string) => (id === 'user' ? 'ты' : agents.find((a) => a.id === id)?.name.split(' ')[0] ?? id)
  const mine = notes.filter((n) => n.node_id === node.id)
  const links = graph.edges.filter((e) => e.source_id === node.id || e.target_id === node.id)
  const color = AGENT_COLORS[node.created_by] ?? '#aaa'

  const commit = () => {
    const patch: { label?: string; body?: string } = {}
    if (label.trim() && label.trim() !== node.label) patch.label = label.trim()
    if (body !== node.body) patch.body = body
    if (Object.keys(patch).length) onPatch(node.id, patch)
    setEditing(false)
  }

  return (
    <div className="flex h-full w-full flex-col border-r border-line bg-void/95 backdrop-blur md:w-[380px]">
      <div className="flex items-center gap-2 border-b border-line px-3 py-2">
        <span className="font-mono text-[10px] uppercase tracking-widest" style={{ color }}>
          {NODE_TYPE_LABEL[node.node_type] ?? node.node_type} · {name(node.created_by)}
        </span>
        <select
          value={node.node_type}
          onChange={(e) => onPatch(node.id, { type: e.target.value })}
          className="ml-1 rounded border border-line bg-panel px-1 py-0.5 font-mono text-[10px] text-zinc-300"
          title="Тип узла"
        >
          {Object.entries(NODE_TYPE_LABEL).map(([k, v]) => (
            <option key={k} value={k}>
              {v}
            </option>
          ))}
        </select>
        <div className="ml-auto flex items-center gap-1">
          <button onClick={() => onPatch(node.id, { pinned: node.pinned === 0 })} className="rounded p-1 text-zinc-500 hover:text-zinc-200" title={node.pinned ? 'Открепить' : 'Закрепить (не двигается)'}>
            {node.pinned ? <PinOff size={14} /> : <Pin size={14} />}
          </button>
          <button onClick={() => setEditing((v) => !v)} className={`rounded p-1 hover:text-zinc-200 ${editing ? 'text-acid' : 'text-zinc-500'}`} title="Редактировать">
            {editing ? <Eye size={14} /> : <Pencil size={14} />}
          </button>
          <button
            onClick={() => {
              if (confirm(`Удалить узел «${node.label}» со всеми пометками и связями?`)) {
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
                  {e.relation_label && <span className="font-mono text-[10px] text-zinc-500">{e.relation_label}</span>}
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
      </div>
    </div>
  )
}
