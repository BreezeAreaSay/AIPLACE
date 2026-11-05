import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  Background,
  BackgroundVariant,
  Controls,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
  type Connection,
  type Edge,
  type NodeChange,
  type NodeMouseHandler,
} from '@xyflow/react'
import { Circle, Download, LayoutGrid, Plus, Search } from 'lucide-react'
import type { Agent, Graph, NodeNote, ObjectType } from '../types'
import { AGENT_COLORS } from '../types'
import { NodeCard, type NoteNodeType } from './NodeCard'
import { DotNode, type DotNodeType } from './DotNode'
import { api } from '../api'

interface Props {
  graph: Graph
  notes: NodeNote[]
  agents: Agent[]
  types: ObjectType[]
  selectedId: string | null
  hotIds: Set<string>
  onSelect: (id: string | null) => void
  onCreate: (label: string, opts: { x: number; y: number }) => void
  onMove: (id: string, x: number, y: number) => void
  onConnect: (source: string, target: string) => void
  /** Shift the toolbar right when a side panel covers the left edge. */
  toolbarOffset?: number
}

const nodeTypes = { note: NodeCard, dot: DotNode }

function CanvasInner({ graph, notes, agents, types, selectedId, hotIds, onSelect, onCreate, onMove, onConnect, toolbarOffset = 0 }: Props) {
  const rf = useReactFlow()
  const [overrides, setOverrides] = useState<Record<string, { x: number; y: number }>>({})
  const [query, setQuery] = useState('')
  const [author, setAuthor] = useState<string>('all')
  const [mode, setMode] = useState<'dots' | 'cards'>(() => (localStorage.getItem('triad.graph.mode') as 'dots' | 'cards') || 'dots')
  const fitted = useRef(false)
  const degree = useMemo(() => {
    const d: Record<string, number> = {}
    for (const e of graph.edges) {
      d[e.source_id] = (d[e.source_id] ?? 0) + 1
      d[e.target_id] = (d[e.target_id] ?? 0) + 1
    }
    return d
  }, [graph.edges])

  const counts = useMemo(() => {
    const m: Record<string, Record<string, number>> = {}
    for (const n of notes) {
      m[n.node_id] ??= {}
      m[n.node_id][n.author] = (m[n.node_id][n.author] ?? 0) + 1
    }
    return m
  }, [notes])

  const q = query.trim().toLowerCase()
  const visible = useMemo(
    () =>
      graph.nodes.filter(
        (n) => n.archived === 0 && (author === 'all' || n.created_by === author) && (!q || n.label.toLowerCase().includes(q) || n.body.toLowerCase().includes(q)),
      ),
    [graph.nodes, author, q],
  )
  const tmap = useMemo(() => Object.fromEntries(types.map((t) => [t.id, t])), [types])
  const visibleIds = useMemo(() => new Set(visible.map((n) => n.id)), [visible])

  const SCALE = mode === 'dots' ? 0.4 : 1
  const nodes: (NoteNodeType | DotNodeType)[] = useMemo(
    () =>
      visible.map((n) => mode === 'dots' ? ({
        id: n.id,
        type: 'dot',
        position: overrides[n.id] ?? { x: (n.x ?? 0) * SCALE, y: (n.y ?? 0) * SCALE },
        data: {
          label: n.label,
          color: AGENT_COLORS[n.created_by] ?? '#9a9ca8',
          typeIcon: tmap[n.type_id]?.icon ?? '◇',
          degree: degree[n.id] ?? 0,
          selected: n.id === selectedId,
          hot: hotIds.has(n.id),
          author: n.created_by,
        },
        draggable: n.pinned === 0,
      } as DotNodeType) : ({
        id: n.id,
        type: 'note',
        position: overrides[n.id] ?? { x: n.x ?? 0, y: n.y ?? 0 },
        data: {
          label: n.label,
          body: n.body,
          nodeType: n.node_type,
          typeIcon: tmap[n.type_id]?.icon ?? '◇',
          typeName: tmap[n.type_id]?.name ?? n.type_id,
          typeColor: tmap[n.type_id]?.color ?? '#9a9ca8',
          author: n.created_by,
          pinned: n.pinned !== 0,
          noteCounts: counts[n.id] ?? {},
          selected: n.id === selectedId,
          hot: hotIds.has(n.id),
        },
        draggable: n.pinned === 0,
      } as NoteNodeType)),
    [visible, overrides, counts, selectedId, hotIds, tmap, mode, degree, SCALE],
  )
  // Position overrides are in the current scale; drop them when the scale changes.
  useEffect(() => {
    setOverrides({})
    setTimeout(() => rf.fitView({ padding: 0.2, duration: 250, maxZoom: mode === 'dots' ? 1.6 : 0.9 }), 60)
  }, [mode, rf])

  const edges: Edge[] = useMemo(
    () =>
      graph.edges
        .filter((e) => visibleIds.has(e.source_id) && visibleIds.has(e.target_id))
        .map((e) => ({
          id: String(e.id),
          source: e.source_id,
          target: e.target_id,
          label: e.relation_label || (e.kind !== 'related' ? e.kind : undefined),
          type: mode === 'dots' ? 'straight' : 'default',
          style: { stroke: `${AGENT_COLORS[e.created_by] ?? '#8a8ca0'}${mode === 'dots' ? 'bb' : '99'}`, strokeWidth: mode === 'dots' ? 1.2 : 1 },
        })),
    [graph.edges, visibleIds, mode],
  )

  useEffect(() => {
    if (!fitted.current && graph.nodes.length > 0) {
      fitted.current = true
      setTimeout(() => rf.fitView({ padding: 0.25, duration: 300, maxZoom: 0.9 }), 50)
    }
  }, [graph.nodes.length, rf])

  const onNodesChange = useCallback((changes: NodeChange<NoteNodeType | DotNodeType>[]) => {
    setOverrides((o) => {
      let next = o
      for (const c of changes) {
        if (c.type === 'position' && c.position) {
          next = { ...next, [c.id]: c.position }
        }
      }
      return next
    })
  }, [])

  const onNodeDragStop = useCallback(
    (_: unknown, node: NoteNodeType | DotNodeType) => {
      onMove(node.id, Math.round(node.position.x / SCALE), Math.round(node.position.y / SCALE))
    },
    [onMove, SCALE],
  )

  const onNodeClick: NodeMouseHandler<NoteNodeType | DotNodeType> = useCallback((_, node) => onSelect(node.id), [onSelect])

  const onPaneClick = useCallback(
    (ev: React.MouseEvent) => {
      if (ev.detail === 2) {
        const p = rf.screenToFlowPosition({ x: ev.clientX, y: ev.clientY })
        onCreate('Новая заметка', { x: Math.round(p.x / SCALE), y: Math.round(p.y / SCALE) })
      } else {
        onSelect(null)
      }
    },
    [rf, onCreate, onSelect],
  )

  const addAtCenter = () => {
    const el = document.querySelector('.react-flow') as HTMLElement | null
    const r = el?.getBoundingClientRect()
    const p = rf.screenToFlowPosition({ x: (r?.left ?? 0) + (r?.width ?? 800) / 2, y: (r?.top ?? 0) + (r?.height ?? 600) / 2 })
    onCreate('Новая заметка', { x: Math.round(p.x / SCALE), y: Math.round(p.y / SCALE) })
  }

  const handleConnect = useCallback(
    (c: Connection) => {
      if (c.source && c.target && c.source !== c.target) onConnect(c.source, c.target)
    },
    [onConnect],
  )

  return (
    <div className="relative h-full w-full">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        onNodeDragStop={onNodeDragStop}
        onNodeClick={onNodeClick}
        onPaneClick={onPaneClick}
        onConnect={handleConnect}
        deleteKeyCode={null}
        minZoom={0.15}
        maxZoom={2}
        proOptions={{ hideAttribution: true }}
        fitView
        fitViewOptions={{ padding: 0.2, maxZoom: mode === 'dots' ? 1.6 : 0.9 }}
      >
        <Background variant={BackgroundVariant.Dots} gap={24} size={1} color="#23242e" />
        <Controls position="bottom-left" showInteractive={false} />
        {graph.nodes.length > 6 && (
          <MiniMap
            position="bottom-right"
            pannable
            zoomable
            style={{ width: 150, height: 100 }}
            nodeColor={(n) => AGENT_COLORS[(n as NoteNodeType | DotNodeType).data.author] ?? '#555'}
            nodeStrokeWidth={6}
            maskColor="rgba(13,14,18,0.6)"
            className="hidden md:block"
          />
        )}
      </ReactFlow>

      {/* Toolbar */}
      <div className="pointer-events-none absolute top-3 flex flex-wrap items-center gap-2 transition-[left]" style={{ left: 12 + toolbarOffset }}>
        <div className="pointer-events-auto flex items-center gap-1.5 rounded border border-line bg-void/90 px-2 py-1 backdrop-blur">
          <Search size={12} className="text-zinc-500" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="поиск по доске"
            className="w-36 bg-transparent font-mono text-[11px] text-zinc-200 placeholder:text-zinc-600 focus:outline-none"
          />
        </div>
        <div className="pointer-events-auto flex overflow-hidden rounded border border-line bg-void/90 font-mono text-[10px] backdrop-blur">
          {[
            ['all', 'все'],
            ['user', 'я'],
            ...agents.map((a) => [a.id, a.name.split(' ')[0]] as [string, string]),
          ].map(([id, label]) => (
            <button
              key={id}
              onClick={() => setAuthor(id)}
              className={`px-2 py-1 ${author === id ? 'bg-zinc-800 text-zinc-100' : 'text-zinc-500 hover:text-zinc-200'}`}
              style={author === id && id !== 'all' ? { color: AGENT_COLORS[id] } : undefined}
            >
              {label}
            </button>
          ))}
        </div>
        <button
          onClick={() => {
            const next = mode === 'dots' ? 'cards' : 'dots'
            setMode(next)
            localStorage.setItem('triad.graph.mode', next)
          }}
          className="pointer-events-auto flex items-center gap-1 rounded border border-line bg-void/90 px-2 py-1 font-mono text-[11px] text-zinc-400 backdrop-blur hover:text-zinc-100"
          title={mode === 'dots' ? 'Показать карточки' : 'Показать точки'}
        >
          {mode === 'dots' ? <LayoutGrid size={12} /> : <Circle size={12} />} {mode === 'dots' ? 'карточки' : 'точки'}
        </button>
        <button
          onClick={addAtCenter}
          className="pointer-events-auto flex items-center gap-1 rounded border border-acid/60 bg-void/90 px-2 py-1 font-mono text-[11px] text-acid backdrop-blur hover:bg-acid/10"
          title="Новая заметка (или двойной клик по холсту)"
        >
          <Plus size={12} /> заметка
        </button>
        <a
          href={api.exportUrl()}
          download
          className="pointer-events-auto flex items-center gap-1 rounded border border-line bg-void/90 px-2 py-1 font-mono text-[11px] text-zinc-400 backdrop-blur hover:text-zinc-100"
          title="Экспорт доски в Markdown"
        >
          <Download size={12} /> .md
        </a>
      </div>

      {graph.nodes.length === 0 && (
        <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
          <div className="max-w-sm rounded border border-dashed border-line bg-void/80 px-5 py-4 text-center font-mono text-xs text-zinc-500">
            <div className="mb-1 text-zinc-300">Доска пуста.</div>
            Двойной клик — своя заметка. Скажи что-нибудь в сцену справа — Режиссёр вытащит узлы сам, а агенты начнут оставлять на них пометки.
          </div>
        </div>
      )}
    </div>
  )
}

export function Canvas(props: Props) {
  return (
    <ReactFlowProvider>
      <CanvasInner {...props} />
    </ReactFlowProvider>
  )
}
