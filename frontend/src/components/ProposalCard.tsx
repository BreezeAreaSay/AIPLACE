import { useState } from 'react'
import { Check, X } from 'lucide-react'
import type { Agent, GraphNode, ObjectType, Proposal } from '../types'
import { AGENT_COLORS } from '../types'

interface Props {
  proposal: Proposal
  agents: Agent[]
  nodes: GraphNode[]
  types: ObjectType[]
  onAccept: (id: number) => Promise<unknown>
  onReject: (id: number) => Promise<unknown>
  onOpen?: (id: string) => void
  compact?: boolean
}

const KIND_LABEL: Record<string, string> = {
  merge: 'слить дубли',
  archive: 'в архив',
  tag: 'теги',
  link: 'связать',
  create: 'завести',
  new_type: 'новый тип',
  retitle: 'переименовать',
  retype: 'сменить тип',
}

/** One proposal from an inhabitant: what changes, why, accept / reject. */
export function ProposalCard({ proposal: p, agents, nodes, types, onAccept, onReject, onOpen, compact }: Props) {
  const [busy, setBusy] = useState<'accept' | 'reject' | null>(null)
  const [err, setErr] = useState<string | null>(null)
  const agent = agents.find((a) => a.id === p.agent_id)
  const color = agent?.color ?? AGENT_COLORS[p.agent_id] ?? '#888'
  const label = (id: unknown) => {
    const n = nodes.find((x) => x.id === id)
    return n ? (
      <button onClick={() => onOpen?.(n.id)} className="text-zinc-100 underline-offset-2 hover:underline">
        {types.find((t) => t.id === n.type_id)?.icon ?? '◇'} {n.label}
      </button>
    ) : (
      <span className="text-zinc-300">{String(id ?? '?')}</span>
    )
  }
  const pl = p.payload
  const what = (() => {
    switch (p.kind) {
      case 'merge':
        return (
          <>
            слить {label(pl.src)} → в {label(pl.dst)}
          </>
        )
      case 'archive':
        return <>убрать в архив {label(pl.id)}</>
      case 'tag':
        return (
          <>
            теги {label(pl.id)}: {(pl.tags as string[] | undefined)?.map((t) => `#${t}`).join(' ')}
          </>
        )
      case 'link':
        return (
          <>
            связать {label(pl.from)} ⇄ {label(pl.to)} <span className="text-zinc-500">({String(pl.kind ?? 'related')})</span>
          </>
        )
      case 'create': {
        const t = types.find((x) => x.id === pl.type)
        return (
          <>
            завести {t?.icon ?? '◇'} <span className="text-zinc-100">{String(pl.title)}</span> <span className="text-zinc-500">({t?.name ?? String(pl.type)})</span>
          </>
        )
      }
      case 'new_type': {
        const props = (pl.props as { label?: string; key?: string }[] | undefined) ?? []
        return (
          <>
            новый тип {String(pl.icon ?? '◇')} <span className="text-zinc-100">{String(pl.name)}</span>
            {props.length > 0 && <span className="text-zinc-500"> · поля: {props.map((x) => x.label ?? x.key).join(', ')}</span>}
          </>
        )
      }
      case 'retitle':
        return (
          <>
            переименовать {label(pl.id)} → «{String(pl.title)}»
          </>
        )
      case 'retype':
        return (
          <>
            {label(pl.id)} → тип {types.find((x) => x.id === pl.type)?.name ?? String(pl.type)}
          </>
        )
      default:
        return <span className="font-mono text-[10px]">{JSON.stringify(pl)}</span>
    }
  })()
  const run = async (which: 'accept' | 'reject') => {
    setBusy(which)
    setErr(null)
    try {
      await (which === 'accept' ? onAccept(p.id) : onReject(p.id))
    } catch (e) {
      setErr((e as Error).message)
    } finally {
      setBusy(null)
    }
  }
  return (
    <div className={`rounded border border-dashed bg-panel/60 ${compact ? 'px-2 py-1' : 'px-2.5 py-1.5'} text-[11px]`} style={{ borderColor: `${color}77` }}>
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <span className="font-mono text-[9px] uppercase tracking-wider" style={{ color }}>
          {KIND_LABEL[p.kind] ?? p.kind}
        </span>
        <span className="min-w-0 flex-1 text-zinc-300">{what}</span>
        <button onClick={() => void run('accept')} disabled={!!busy} className="flex items-center gap-1 rounded bg-acid px-2 py-0.5 font-mono text-[10px] font-bold text-void disabled:opacity-40" title="Принять">
          <Check size={10} /> {busy === 'accept' ? '…' : 'принять'}
        </button>
        <button onClick={() => void run('reject')} disabled={!!busy} className="flex items-center gap-1 rounded border border-line px-2 py-0.5 font-mono text-[10px] text-zinc-400 hover:border-magenta hover:text-magenta disabled:opacity-40" title="Отклонить">
          <X size={10} /> нет
        </button>
      </div>
      {p.reason && !compact && <div className="mt-0.5 text-[10px] text-zinc-500">{p.reason}</div>}
      {err && <div className="mt-0.5 text-[10px] text-magenta">{err}</div>}
    </div>
  )
}
