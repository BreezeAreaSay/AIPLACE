import { useEffect, useMemo, useRef, useState } from 'react'
import { Command, CornerDownLeft, FileText, MessageSquare, Search, Zap } from 'lucide-react'
import type { Agent, GraphNode, Message, ObjectType } from '../types'
import { AGENT_COLORS } from '../types'
import { api } from '../api'

export interface PaletteCommand {
  id: string
  title: string
  hint?: string
  keys?: string
  run: () => void
}

interface Props {
  open: boolean
  onClose: () => void
  nodes: GraphNode[]
  types: ObjectType[]
  agents: Agent[]
  commands: PaletteCommand[]
  onOpenNode: (id: string) => void
  onJumpMessage: (id: number) => void
}

type Row = { kind: 'cmd'; cmd: PaletteCommand } | { kind: 'node'; node: GraphNode } | { kind: 'msg'; msg: Message }

/** Ctrl+K: objects, messages and actions in one search box. */
export function CommandPalette({ open, onClose, nodes, types, agents, commands, onOpenNode, onJumpMessage }: Props) {
  const [q, setQ] = useState('')
  const [msgs, setMsgs] = useState<Message[]>([])
  const [sel, setSel] = useState(0)
  const input = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (open) {
      setQ('')
      setSel(0)
      setMsgs([])
      setTimeout(() => input.current?.focus(), 30)
    }
  }, [open])

  useEffect(() => {
    if (!open || q.trim().length < 2 || q.startsWith('>')) {
      setMsgs([])
      return
    }
    const t = setTimeout(() => void api.searchMessages(q).then((m) => setMsgs(m.slice(-6).reverse())).catch(() => setMsgs([])), 200)
    return () => clearTimeout(t)
  }, [q, open])

  const rows = useMemo<Row[]>(() => {
    const ql = q.trim().toLowerCase().replace(/^>\s*/, '')
    const cmdOnly = q.startsWith('>')
    const cmds: Row[] = commands.filter((c) => !ql || c.title.toLowerCase().includes(ql) || (c.hint ?? '').toLowerCase().includes(ql)).slice(0, cmdOnly ? 30 : ql ? 5 : 8).map((cmd) => ({ kind: 'cmd', cmd }))
    if (cmdOnly) return cmds
    const ns: Row[] = (ql ? nodes.filter((n) => n.archived === 0 && (n.label.toLowerCase().includes(ql) || n.id.includes(ql) || n.body.toLowerCase().includes(ql))) : nodes.filter((n) => n.archived === 0).sort((a, b) => b.updated_at.localeCompare(a.updated_at)).slice(0, 6))
      .slice(0, 8)
      .map((node) => ({ kind: 'node', node }))
    const ms: Row[] = msgs.map((msg) => ({ kind: 'msg', msg }))
    return [...ns, ...ms, ...cmds]
  }, [q, nodes, msgs, commands])

  useEffect(() => setSel(0), [rows.length, q])
  if (!open) return null

  const fire = (r: Row) => {
    onClose()
    if (r.kind === 'cmd') r.cmd.run()
    else if (r.kind === 'node') onOpenNode(r.node.id)
    else onJumpMessage(r.msg.id)
  }

  return (
    <div className="fixed inset-0 z-[70] flex items-start justify-center bg-black/70 px-4 pt-[12vh]" onClick={onClose}>
      <div className="w-full max-w-xl overflow-hidden rounded-xl border border-line bg-void shadow-2xl" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center gap-2 border-b border-line px-3 py-2">
          <Search size={14} className="text-zinc-500" />
          <input
            ref={input}
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'ArrowDown') {
                e.preventDefault()
                setSel((s) => Math.min(rows.length - 1, s + 1))
              } else if (e.key === 'ArrowUp') {
                e.preventDefault()
                setSel((s) => Math.max(0, s - 1))
              } else if (e.key === 'Enter' && rows[sel]) {
                fire(rows[sel])
              } else if (e.key === 'Escape') onClose()
            }}
            placeholder="объект, реплика или команда… («>» — только команды)"
            className="flex-1 bg-transparent text-sm text-zinc-100 placeholder:text-zinc-600 focus:outline-none"
          />
          <kbd className="rounded border border-line px-1 font-mono text-[9px] text-zinc-500">esc</kbd>
        </div>
        <div className="max-h-[60vh] overflow-y-auto py-1">
          {rows.length === 0 && <div className="px-3 py-3 font-mono text-[11px] text-zinc-600">ничего</div>}
          {rows.map((r, i) => {
            const active = i === sel
            const base = `flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm ${active ? 'bg-zinc-800 text-zinc-100' : 'text-zinc-300 hover:bg-zinc-900'}`
            if (r.kind === 'node') {
              const t = types.find((x) => x.id === r.node.type_id)
              return (
                <button key={`n${r.node.id}`} onMouseEnter={() => setSel(i)} onClick={() => fire(r)} className={base}>
                  <span className="w-5 text-center" style={{ color: t?.color }}>
                    {t?.icon ?? '◇'}
                  </span>
                  <span className="truncate">{r.node.label}</span>
                  <span className="ml-auto shrink-0 font-mono text-[10px] text-zinc-600">{t?.name}</span>
                </button>
              )
            }
            if (r.kind === 'msg') {
              const who = r.msg.sender_id === 'user' ? 'ты' : agents.find((a) => a.id === r.msg.sender_id)?.name.split(' ')[0] ?? r.msg.sender_id
              return (
                <button key={`m${r.msg.id}`} onMouseEnter={() => setSel(i)} onClick={() => fire(r)} className={base}>
                  <MessageSquare size={13} className="w-5 shrink-0 text-zinc-500" />
                  <span className="shrink-0 font-mono text-[10px]" style={{ color: AGENT_COLORS[r.msg.sender_id] ?? '#888' }}>
                    {who}
                  </span>
                  <span className="truncate text-[12px]">{r.msg.content}</span>
                </button>
              )
            }
            return (
              <button key={`c${r.cmd.id}`} onMouseEnter={() => setSel(i)} onClick={() => fire(r)} className={base}>
                {r.cmd.id.startsWith('new') ? <FileText size={13} className="w-5 shrink-0 text-acid" /> : <Zap size={13} className="w-5 shrink-0 text-acid" />}
                <span>{r.cmd.title}</span>
                {r.cmd.hint && <span className="truncate text-[11px] text-zinc-500">{r.cmd.hint}</span>}
                {r.cmd.keys && <kbd className="ml-auto shrink-0 rounded border border-line px-1 font-mono text-[9px] text-zinc-500">{r.cmd.keys}</kbd>}
              </button>
            )
          })}
        </div>
        <div className="flex items-center gap-3 border-t border-line px-3 py-1 font-mono text-[9px] text-zinc-600">
          <span className="flex items-center gap-1">
            <Command size={9} /> K — открыть
          </span>
          <span className="flex items-center gap-1">
            <CornerDownLeft size={9} /> выбрать
          </span>
          <span>↑↓ ходить</span>
        </div>
      </div>
    </div>
  )
}
