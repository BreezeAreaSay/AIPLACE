import type { Agent, Relation } from '../types'
import { glyphOf } from '../types'

interface Props {
  agents: Agent[]
  relations: Relation[]
  size?: number
}

function affinityColor(v: number) {
  if (v <= -50) return '#FF0055'
  if (v < -15) return '#ff6b8f'
  if (v < 15) return '#6b6d7c'
  if (v < 50) return '#b8e600'
  return '#CCFF00'
}

/** The yard on a circle (three make the classic triangle), edges coloured by how much they can stand each other. */
export function RelationsTriangle({ agents, relations, size = 220 }: Props) {
  const cast = agents.filter((a) => a.active !== 0)
  const ids = cast.map((a) => a.id)
  const cx = size / 2
  const r = size * (ids.length <= 3 ? 0.36 : 0.4)
  const pos: Record<string, { x: number; y: number }> = {}
  ids.forEach((id, i) => {
    const ang = -Math.PI / 2 + (i * 2 * Math.PI) / Math.max(ids.length, 1)
    pos[id] = { x: cx + r * Math.cos(ang), y: cx + 12 + r * Math.sin(ang) }
  })
  const rel = (a: string, b: string) => relations.find((x) => (x.a_id === a && x.b_id === b) || (x.a_id === b && x.b_id === a))
  const pairs: [string, string][] = []
  for (let i = 0; i < ids.length; i++) for (let j = i + 1; j < ids.length; j++) pairs.push([ids[i], ids[j]])
  const showLabels = ids.length <= 4
  return (
    <svg viewBox={`0 0 ${size} ${size + 16}`} width="100%" style={{ maxWidth: size + 40 }} role="img" aria-label="Отношения жителей">
      {pairs.map(([a, b]) => {
        const rr = rel(a, b)
        const v = rr?.affinity ?? 0
        const pa = pos[a]
        const pb = pos[b]
        const mx = (pa.x + pb.x) / 2
        const my = (pa.y + pb.y) / 2
        return (
          <g key={a + b}>
            <line x1={pa.x} y1={pa.y} x2={pb.x} y2={pb.y} stroke={affinityColor(v)} strokeWidth={1 + Math.abs(v) / 30} strokeOpacity={0.9}>
              <title>{rr?.label || '—'}</title>
            </line>
            {(showLabels || Math.abs(v) >= 25) && (
              <>
                <rect x={mx - 14} y={my - 7} width={28} height={14} rx={3} fill="#0D0E12" stroke={affinityColor(v)} strokeWidth={0.8} />
                <text x={mx} y={my + 3.5} textAnchor="middle" fontSize={9} fontFamily="ui-monospace, monospace" fill={affinityColor(v)}>
                  {v > 0 ? `+${v}` : v}
                </text>
              </>
            )}
          </g>
        )
      })}
      {cast.map((a) => {
        const p = pos[a.id]
        const dead = a.hp <= 0
        const color = dead ? '#55565f' : a.color
        return (
          <g key={a.id}>
            <rect x={p.x - 14} y={p.y - 14} width={28} height={28} rx={4} fill="#13141a" stroke={color} strokeWidth={2} />
            <text x={p.x} y={p.y + 5} textAnchor="middle" fontSize={14} fontWeight={700} fill={color}>
              {dead ? '☠' : glyphOf(a)}
            </text>
            <text x={p.x} y={p.y + 28} textAnchor="middle" fontSize={9} fontFamily="ui-monospace, monospace" fill="#9a9ca8">
              {a.name.split(' ')[0]}
            </text>
          </g>
        )
      })}
    </svg>
  )
}
