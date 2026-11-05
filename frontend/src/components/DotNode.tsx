import { memo } from 'react'
import { Handle, Position, type NodeProps, type Node } from '@xyflow/react'

export type DotData = {
  label: string
  color: string
  typeIcon: string
  degree: number
  selected: boolean
  hot: boolean
  author: string
}

export type DotNodeType = Node<DotData, 'dot'>

/** Obsidian-style graph node: a small circle sized by degree, label underneath. */
function DotNodeInner({ data }: NodeProps<DotNodeType>) {
  const r = 7 + Math.min(data.degree, 8) * 1.6
  return (
    <div className="relative flex flex-col items-center" style={{ width: 120, marginLeft: -60 + r, marginTop: -r }}>
      <Handle type="target" position={Position.Left} style={{ opacity: 0, left: 60, top: r, pointerEvents: 'none' }} />
      <div
        className="rounded-full transition-shadow"
        style={{
          width: r * 2,
          height: r * 2,
          background: data.selected ? data.color : `${data.color}cc`,
          boxShadow: data.hot ? `0 0 18px ${data.color}` : data.selected ? `0 0 12px ${data.color}aa` : `0 0 4px ${data.color}55`,
          border: `1.5px solid ${data.color}`,
        }}
        title={data.label}
      />
      <div className="mt-1 max-w-[120px] truncate text-center font-mono text-[10px] leading-tight text-zinc-300" style={{ textShadow: '0 0 4px #0D0E12' }}>
        {data.typeIcon} {data.label}
      </div>
      <Handle type="source" position={Position.Right} style={{ opacity: 0, left: 60, top: r, pointerEvents: 'none' }} />
    </div>
  )
}

export const DotNode = memo(DotNodeInner)
