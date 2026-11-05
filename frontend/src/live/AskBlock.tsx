import { useState } from 'react'
import { MessageSquare } from 'lucide-react'
import { Markdown } from '../components/Markdown'
import { AGENT_COLORS } from '../types'
import { api } from '../api'
import { useLive, useNoteScope } from './LiveContext'

/**
 * ```ask Вера: что я тут избегаю?
 * Previous answers live inside the block as lines "@agent_id: text" so the note keeps them.
 */
export function AskBlock({ src, onReplace }: { src: string; onReplace?: (next: string) => void }) {
  const live = useLive()
  const { nodeId } = useNoteScope()
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState<string | null>(null)
  const lines = src.split('\n')
  const head = lines[0].replace(/^ask\s*/i, '').trim()
  let agentId: string | null = null
  let question = head
  const m = head.match(/^([^:]{2,40}):\s*(.+)$/)
  if (m && live) {
    const who = m[1].trim().toLowerCase()
    const a = live.agents.find((x) => x.id === who || x.name.toLowerCase().startsWith(who) || x.name.toLowerCase().split(' ')[0] === who)
    if (a) {
      agentId = a.id
      question = m[2].trim()
    }
  }
  const answers = lines.slice(1).join('\n').split(/\n(?=@\w+:)/).map((s) => s.trim()).filter((s) => /^@\w+:/.test(s)).map((s) => {
    const mm = s.match(/^@(\w+):\s*([\s\S]*)$/)!
    return { agent: mm[1], text: mm[2] }
  })
  const ask = async () => {
    if (!nodeId || !question) return
    setBusy(true)
    setErr(null)
    try {
      const r = await api.askInline(nodeId, question, agentId ?? undefined)
      const next = `${lines[0]}\n${lines.slice(1).join('\n').trim()}${answers.length ? '\n' : ''}@${r.agent}: ${r.answer.trim()}`.replace(/\n{3,}/g, '\n\n')
      onReplace?.(next)
    } catch (e) {
      setErr((e as Error).message)
    } finally {
      setBusy(false)
    }
  }
  const name = (id: string) => live?.agents.find((a) => a.id === id)?.name.split(' ')[0] ?? id
  return (
    <div className="my-2 rounded border border-dashed border-cyan/40 bg-cyan/5 px-3 py-2">
      <div className="flex items-start gap-2">
        <MessageSquare size={13} className="mt-0.5 shrink-0 text-cyan" />
        <div className="min-w-0 flex-1 text-[13px] text-zinc-200">
          {agentId && <span className="mr-1 font-mono text-[10px]" style={{ color: AGENT_COLORS[agentId] }}>{name(agentId)}, </span>}
          {question || <span className="text-zinc-600">вопрос пустой</span>}
        </div>
        <button onClick={() => void ask()} disabled={busy || !nodeId || !question || !onReplace} className="shrink-0 rounded border border-cyan/50 px-2 py-0.5 font-mono text-[10px] text-cyan hover:bg-cyan/10 disabled:opacity-40" title={!nodeId ? 'работает внутри заметки' : 'ответ сохранится в заметке'}>
          {busy ? 'думает…' : answers.length ? 'спросить ещё' : 'спросить'}
        </button>
      </div>
      {answers.map((a, i) => (
        <div key={i} className="mt-2 border-l-2 pl-2" style={{ borderColor: AGENT_COLORS[a.agent] ?? '#666' }}>
          <div className="font-mono text-[10px]" style={{ color: AGENT_COLORS[a.agent] ?? '#888' }}>
            {name(a.agent)}
          </div>
          <div className="text-[13px]">
            <Markdown text={a.text} nodes={live?.nodes ?? []} types={live?.types ?? []} onOpen={live?.actions.openNode} variant="chat" />
          </div>
        </div>
      ))}
      {err && <div className="mt-1 text-[10px] text-magenta">{err}</div>}
    </div>
  )
}
