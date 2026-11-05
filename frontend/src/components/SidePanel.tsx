import type { Agent, AgentMemory, Contract, DirectorLogEntry, Graph, GraphNode, NodeNote, ObjectType, Relation, RoomEventRecord } from '../types'
import { Library } from './Library'
import { NodePanel } from './NodePanel'
import { Canvas } from './Canvas'
import { ContractsPanel } from './ContractsPanel'
import { DirectorInspector } from './DirectorInspector'

export type SideTab = 'library' | 'object' | 'graph' | 'contracts' | 'director'

interface Props {
  tab: SideTab
  onTab: (t: SideTab) => void
  graph: Graph
  types: ObjectType[]
  notes: NodeNote[]
  agents: Agent[]
  selected: GraphNode | null
  hotIds: Set<string>
  disabled: boolean
  onSelect: (id: string | null) => void
  onCreate: (label: string, opts: { type_id?: string; body?: string; props?: Record<string, unknown>; x?: number; y?: number }) => void
  onImport: (files: { name: string; content: string }[]) => Promise<{ imported: number; updated: number; linked: number }>
  libraryQuery: { q: string; nonce: number } | null
  onCreateType: (name: string) => void
  onPatch: (id: string, patch: Record<string, unknown>) => void
  onDelete: (id: string) => void
  onAsk: (id: string, mode: 'discuss' | 'develop' | 'dispute', message?: string) => void
  onAddNote: (id: string, text: string) => void
  onDeleteEdge: (id: number) => void
  onConnect: (a: string, b: string) => void
  onMove: (id: string, x: number, y: number) => void
  contracts: Contract[]
  now: string
  onResolve: (id: number, status: 'done' | 'failed' | 'cancelled') => void
  onPromise: (text: string) => void
  director: DirectorLogEntry | null
  directorModel: string
  relations: Relation[]
  memories: AgentMemory[]
  events: RoomEventRecord[]
}

export function SidePanel(p: Props) {
  const openContracts = p.contracts.filter((c) => c.status === 'open').length
  const live = p.graph.nodes.filter((n) => n.archived === 0).length
  const tabs: [SideTab, string][] = [
    ['library', `Библиотека · ${live}`],
    ['object', p.selected ? `◇ ${p.selected.label.length > 18 ? p.selected.label.slice(0, 18) + '…' : p.selected.label}` : 'Объект'],
    ['graph', 'Граф'],
    ['contracts', `Слово${openContracts ? ` · ${openContracts}` : ''}`],
    ['director', 'Режиссёр'],
  ]
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex items-center gap-0.5 overflow-x-auto border-b border-line px-1">
        {tabs.map(([id, label]) => (
          <button
            key={id}
            onClick={() => p.onTab(id)}
            disabled={id === 'object' && !p.selected}
            className={`shrink-0 truncate border-b-2 px-2 py-1.5 font-mono text-[11px] transition disabled:opacity-40 ${p.tab === id ? 'border-acid text-acid' : 'border-transparent text-zinc-500 hover:text-zinc-300'}`}
          >
            {label}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1">
        {p.tab === 'library' && (
          <Library
            nodes={p.graph.nodes}
            types={p.types}
            agents={p.agents}
            selectedId={p.selected?.id ?? null}
            externalQuery={p.libraryQuery}
            onSelect={(id) => p.onSelect(id)}
            onCreate={(label, type_id) => p.onCreate(label, { type_id })}
            onCreateType={p.onCreateType}
            onImport={p.onImport}
          />
        )}
        {p.tab === 'object' && p.selected && (
          <NodePanel
            node={p.selected}
            graph={p.graph}
            types={p.types}
            notes={p.notes}
            agents={p.agents}
            disabled={p.disabled}
            onClose={() => {
              p.onSelect(null)
              p.onTab('library')
            }}
            onPatch={p.onPatch}
            onDelete={p.onDelete}
            onAsk={p.onAsk}
            onAddNote={p.onAddNote}
            onDeleteEdge={p.onDeleteEdge}
            onSelect={(id) => p.onSelect(id)}
            onCreate={(label, opts) => p.onCreate(label, opts)}
          />
        )}
        {p.tab === 'object' && !p.selected && <div className="p-4 font-mono text-xs text-zinc-600">Выбери объект в библиотеке, на графе или по чипу в чате.</div>}
        {p.tab === 'graph' && (
          <Canvas graph={p.graph} notes={p.notes} agents={p.agents} types={p.types} selectedId={p.selected?.id ?? null} hotIds={p.hotIds} onSelect={p.onSelect} onCreate={(label, pos) => p.onCreate(label, pos)} onMove={p.onMove} onConnect={p.onConnect} />
        )}
        {p.tab === 'contracts' && (
          <div className="h-full overflow-y-auto">
            <ContractsPanel contracts={p.contracts} agents={p.agents} now={p.now} disabled={p.disabled} onResolve={p.onResolve} onPromise={p.onPromise} />
          </div>
        )}
        {p.tab === 'director' && (
          <div className="h-full overflow-y-auto">
            <DirectorInspector entry={p.director} agents={p.agents} directorModel={p.directorModel} relations={p.relations} memories={p.memories} events={p.events} />
          </div>
        )}
      </div>
    </div>
  )
}
