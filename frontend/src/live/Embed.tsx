import { Markdown } from '../components/Markdown'
import { api } from '../api'
import { NoteScope, useLive, useNoteScope } from './LiveContext'

/** `![[Объект]]` — the note inline; `![[файл.pdf]]` — the attachment inline. */
export function Embed({ target }: { target: string }) {
  const live = useLive()
  const { depth } = useNoteScope()
  if (!live) return <span className="font-mono text-[10px] text-zinc-600">[[{target}]]</span>
  const t = target.trim().toLowerCase()
  const att = live.attachments.find((a) => a.name.toLowerCase() === t || a.id === target)
  if (att) {
    const url = api.fileUrl(att.id)
    if (att.mime.startsWith('image/')) return <img src={url} alt={att.name} className="max-h-96 rounded border border-line" />
    if (att.mime === 'application/pdf') return <iframe src={url} title={att.name} className="h-96 w-full rounded border border-line bg-white" />
    if (att.mime.startsWith('audio/')) return <audio controls src={url} className="w-full" />
    if (att.mime.startsWith('video/')) return <video controls src={url} className="max-h-96 w-full rounded border border-line" />
    return (
      <a href={url} target="_blank" rel="noreferrer" className="text-cyan underline">
        📎 {att.name}
      </a>
    )
  }
  const node = live.nodes.find((n) => n.id === t) ?? live.nodes.find((n) => n.label.toLowerCase() === t)
  if (!node) return <span className="wiki-missing font-mono text-[11px]">![[{target}]] — нет такого объекта</span>
  const type = live.types.find((x) => x.id === node.type_id)
  if (depth >= 2) {
    return (
      <button onClick={() => live.actions.openNode(node.id)} className="wiki">
        {type?.icon} {node.label}
      </button>
    )
  }
  return (
    <div className="my-2 rounded border border-line bg-panel/30">
      <button onClick={() => live.actions.openNode(node.id)} className="flex w-full items-center gap-2 border-b border-line px-2 py-1 text-left text-[12px] text-zinc-200 hover:bg-zinc-900">
        <span style={{ color: type?.color }}>{type?.icon ?? '◇'}</span>
        <span className="font-semibold">{node.label}</span>
        <span className="ml-auto font-mono text-[9px] text-zinc-600">{type?.name} · открыть →</span>
      </button>
      <div className="px-3 py-2 text-[13px]">
        <NoteScope nodeId={node.id} depth={depth + 1}>
          {node.body.trim() ? <Markdown text={node.body} nodes={live.nodes} types={live.types} onOpen={live.actions.openNode} /> : <span className="text-[11px] text-zinc-600">пусто</span>}
        </NoteScope>
      </div>
    </div>
  )
}
