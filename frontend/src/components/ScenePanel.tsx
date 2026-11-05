import { useEffect, useRef, useState } from 'react'
import type { Agent, AgentMemory, Contract, DirectorLogEntry, GraphNode, Message, Relation, RoomEventRecord } from '../types'
import { AgentStrip } from './AgentStrip'
import { MessageBubble } from './MessageBubble'
import { Composer } from './Composer'
import { ContractsPanel } from './ContractsPanel'
import { DirectorInspector } from './DirectorInspector'
import type { Busy } from '../hooks/useRoom'

type Tab = 'scene' | 'contracts' | 'director'

interface Props {
  agents: Agent[]
  messages: Message[]
  nodes: GraphNode[]
  busy: Busy
  speaking: string | null
  showEgo: boolean
  onToggleEgo: () => void
  disabled: boolean
  onSend: (text: string, nodeId?: string | null) => void
  onTick: () => void
  contextNode: GraphNode | null
  onClearContext: () => void
  onNodeClick: (id: string) => void
  onOpenAgent: (id: string) => void
  contracts: Contract[]
  now: string
  onResolve: (id: number, status: 'done' | 'failed' | 'cancelled') => void
  director: DirectorLogEntry | null
  directorModel: string
  relations: Relation[]
  memories: AgentMemory[]
  events: RoomEventRecord[]
  mood: string | null
}

export function ScenePanel(p: Props) {
  const [tab, setTab] = useState<Tab>('scene')
  const bottom = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (tab === 'scene') bottom.current?.scrollIntoView({ behavior: 'smooth', block: 'end' })
  }, [p.messages.length, p.busy?.detail, tab])
  const openContracts = p.contracts.filter((c) => c.status === 'open').length
  return (
    <div className="flex h-full min-h-0 flex-col">
      <AgentStrip agents={p.agents} speaking={p.speaking} onOpen={p.onOpenAgent} />
      <div className="flex items-center gap-1 border-b border-line px-2">
        {(
          [
            ['scene', p.mood ? `Сцена · ${p.mood}` : 'Сцена'],
            ['contracts', `Слово${openContracts ? ` · ${openContracts}` : ''}`],
            ['director', 'Режиссёр'],
          ] as [Tab, string][]
        ).map(([id, label]) => (
          <button
            key={id}
            onClick={() => setTab(id)}
            className={`truncate border-b-2 px-2 py-1.5 font-mono text-[11px] transition ${tab === id ? 'border-acid text-acid' : 'border-transparent text-zinc-500 hover:text-zinc-300'}`}
          >
            {label}
          </button>
        ))}
      </div>
      {tab === 'scene' && (
        <>
          <div className="min-h-0 flex-1 space-y-3 overflow-y-auto px-3 py-3">
            {p.messages.map((m) => (
              <MessageBubble key={m.id} message={m} agents={p.agents} nodes={p.nodes} showEgo={p.showEgo} onNodeClick={p.onNodeClick} />
            ))}
            {p.busy && (
              <div className="flex items-center gap-2 px-1 font-mono text-[11px] text-zinc-500">
                <span className="inline-block h-1.5 w-1.5 animate-pulseLine rounded-full bg-acid" />
                <span className="animate-pulseLine">{p.busy.detail}</span>
              </div>
            )}
            <div ref={bottom} />
          </div>
          <Composer
            disabled={p.disabled}
            onSend={p.onSend}
            onTick={p.onTick}
            showEgo={p.showEgo}
            onToggleEgo={p.onToggleEgo}
            contextNode={p.contextNode}
            onClearContext={p.onClearContext}
          />
        </>
      )}
      {tab === 'contracts' && (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <ContractsPanel contracts={p.contracts} agents={p.agents} now={p.now} disabled={p.disabled} onResolve={p.onResolve} onPromise={(t) => p.onSend(t)} />
        </div>
      )}
      {tab === 'director' && (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <DirectorInspector entry={p.director} agents={p.agents} directorModel={p.directorModel} relations={p.relations} memories={p.memories} events={p.events} />
        </div>
      )}
    </div>
  )
}
