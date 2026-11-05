import { memo } from 'react'
import { Handle, Position, type NodeProps, type Node } from '@xyflow/react'
import { Pin } from 'lucide-react'
import { AGENT_COLORS } from '../types'

export type NoteData = {
  label: string
  body: string
  nodeType: string
  typeIcon: string
  typeName: string
  typeColor: string
  author: string
  pinned: boolean
  noteCounts: Record<string, number>
  selected: boolean
  hot: boolean
}

export type NoteNodeType = Node<NoteData, 'note'>

function NodeCardInner({ data }: NodeProps<NoteNodeType>) {
  const color = AGENT_COLORS[data.author] ?? '#aaa'
  const preview = data.body.replace(/[#*`>_]/g, '').trim()
  return (
    <div
      className={`w-[260px] rounded-md border bg-panel/95 px-3 py-2 shadow-lg transition-shadow ${data.selected ? 'ring-1' : ''}`}
      style={{
        borderColor: data.selected ? color : `${color}88`,
        boxShadow: data.hot ? `0 0 22px ${color}66` : data.selected ? `0 0 16px ${color}44` : undefined,
        // @ts-expect-error css var for ring color
        '--tw-ring-color': color,
      }}
    >
      <Handle type="target" position={Position.Left} />
      <div className="mb-1 flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-widest" style={{ color: data.typeColor }}>
        <span>{data.typeIcon}</span>
        <span>{data.typeName}</span>
        {data.pinned && <Pin size={10} className="ml-auto text-zinc-400" />}
      </div>
      <div className="text-[13px] font-semibold leading-snug text-zinc-100">{data.label}</div>
      {preview && <div className="mt-1 line-clamp-3 text-[11px] leading-snug text-zinc-400">{preview}</div>}
      <div className="mt-1.5 flex items-center gap-1">
        {Object.entries(data.noteCounts).map(([a, n]) => (
          <span key={a} title={`${n} пометок`} className="flex items-center gap-0.5 font-mono text-[9px]" style={{ color: AGENT_COLORS[a] }}>
            ● {n}
          </span>
        ))}
      </div>
      <Handle type="source" position={Position.Right} />
    </div>
  )
}

export const NodeCard = memo(NodeCardInner)
