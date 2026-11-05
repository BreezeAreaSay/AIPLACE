import { memo, useCallback, useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import {
  Background,
  BackgroundVariant,
  Controls,
  Handle,
  MarkerType,
  MiniMap,
  NodeResizer,
  Position,
  ReactFlow,
  ReactFlowProvider,
  useEdgesState,
  useNodesState,
  useReactFlow,
  type Connection,
  type Edge,
  type EdgeChange,
  type Node,
  type NodeChange,
  type NodeProps,
} from '@xyflow/react'
import { Boxes, ExternalLink, Link2, LayoutGrid, Maximize2, Minimize2, Plus, Search, Type } from 'lucide-react'
import type { GraphEdge, GraphNode, ObjectType } from '../types'
import { Markdown } from './Markdown'

/**
 * Free-form board stored in `props.board` of a `canvas` object:
 * text cards (markdown, live blocks included), object cards (live references to the library),
 * groups (resizable frames) and arrows between them.
 */
export interface BoardItem {
  id: string
  kind: 'text' | 'object' | 'group'
  ref?: string
  text?: string
  x: number
  y: number
  w?: number
  h?: number
  color?: string
}
export interface BoardEdge {
  id: string
  from: string
  to: string
  label?: string
}
export interface Board {
  items: BoardItem[]
  edges: BoardEdge[]
}

interface Props {
  node: GraphNode
  nodes: GraphNode[]
  edges: GraphEdge[]
  types: ObjectType[]
  disabled?: boolean
  onSave: (board: Board) => void
  onOpen: (id: string) => void
  onCreate: (label: string, opts: { type_id?: string; body?: string }) => Promise<unknown> | void
  /** Height class for the embedded mode; fullscreen ignores it. */
  className?: string
}

export function readBoard(v: unknown): Board {
  const b = (v && typeof v === 'object' ? v : {}) as Partial<Board>
  return { items: Array.isArray(b.items) ? b.items : [], edges: Array.isArray(b.edges) ? b.edges : [] }
}

const uid = () => Math.random().toString(36).slice(2, 8)

// ── card renderers ──────────────────────────────────────────────────────────

type TextData = { text: string; onText: (t: string) => void; nodes: GraphNode[]; types: ObjectType[]; onOpen: (id: string) => void; onCreate: (t: string) => void }
type TextNodeType = Node<TextData, 'text'>

const TextCard = memo(function TextCard({ data, selected }: NodeProps<TextNodeType>) {
  const [editing, setEditing] = useState(!data.text)
  const [draft, setDraft] = useState(data.text)
  useEffect(() => {
    if (!editing) setDraft(data.text)
  }, [data.text, editing])
  const done = () => {
    setEditing(false)
    if (draft !== data.text) data.onText(draft)
  }
  return (
    <div className={`h-full w-full rounded-md border bg-panel/95 px-3 py-2 text-[12px] leading-relaxed text-zinc-200 shadow-lg ${selected ? 'border-acid' : 'border-line'}`} onDoubleClick={() => setEditing(true)}>
      <NodeResizer isVisible={selected} minWidth={140} minHeight={60} lineClassName="!border-acid/50" handleClassName="!h-2 !w-2 !border-acid !bg-void" />
      <Handle type="target" position={Position.Left} className="!h-2 !w-2 !border-0 !bg-zinc-500" />
      <Handle type="source" position={Position.Right} className="!h-2 !w-2 !border-0 !bg-acid" />
      {editing ? (
        <textarea
          autoFocus
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={done}
          onKeyDown={(e) => {
            if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') done()
            if (e.key === 'Escape') done()
            e.stopPropagation()
          }}
          placeholder="Текст карточки (markdown, [[ссылки]], живые блоки)…"
          className="nodrag nowheel h-full min-h-[48px] w-full resize-none bg-transparent font-mono text-[11px] text-zinc-100 placeholder:text-zinc-600 focus:outline-none"
        />
      ) : (
        <div className="nowheel h-full overflow-y-auto">
          <Markdown text={data.text} nodes={data.nodes} types={data.types} onOpen={data.onOpen} onCreate={data.onCreate} variant="chat" />
        </div>
      )}
    </div>
  )
})

type ObjectData = { obj: GraphNode | null; type?: ObjectType; onOpen: (id: string) => void }
type ObjectNodeType = Node<ObjectData, 'object'>

const ObjectCard = memo(function ObjectCard({ data, selected }: NodeProps<ObjectNodeType>) {
  const o = data.obj
  const color = data.type?.color ?? '#9a9ca8'
  const preview = (o?.props.summary as string | undefined) || o?.body.replace(/<!--[\s\S]*?-->/g, '').replace(/[#*`>_[\]!]/g, '').trim().slice(0, 160) || ''
  const chips = o
    ? (data.type?.schema.props ?? [])
        .filter((p) => !['summary', 'tags', 'board'].includes(p.key) && o.props[p.key] != null && o.props[p.key] !== '' && o.props[p.key] !== false)
        .slice(0, 3)
        .map((p) => `${p.label}: ${Array.isArray(o.props[p.key]) ? (o.props[p.key] as unknown[]).join(', ') : String(o.props[p.key] === true ? '✓' : o.props[p.key])}`)
    : []
  return (
    <div className={`w-[220px] rounded-md border bg-panel/95 px-3 py-2 shadow-lg ${selected ? 'ring-1 ring-acid' : ''}`} style={{ borderColor: `${color}88` }}>
      <Handle type="target" position={Position.Left} className="!h-2 !w-2 !border-0 !bg-zinc-500" />
      <Handle type="source" position={Position.Right} className="!h-2 !w-2 !border-0 !bg-acid" />
      {o ? (
        <>
          <div className="flex items-start gap-1.5">
            <span className="text-[13px]" style={{ color }}>
              {data.type?.icon ?? '◇'}
            </span>
            <div className="min-w-0 flex-1">
              <div className="truncate text-[12px] font-semibold text-zinc-100">{o.label}</div>
              <div className="font-mono text-[9px] uppercase tracking-wider text-zinc-500">{data.type?.name ?? o.type_id}</div>
            </div>
            <button onClick={() => data.onOpen(o.id)} className="nodrag shrink-0 text-zinc-500 hover:text-acid" title="Открыть">
              <ExternalLink size={11} />
            </button>
          </div>
          {preview && <div className="mt-1 line-clamp-3 text-[11px] leading-snug text-zinc-400">{preview}</div>}
          {chips.length > 0 && (
            <div className="mt-1 flex flex-wrap gap-1">
              {chips.map((c) => (
                <span key={c} className="rounded border border-line px-1 font-mono text-[9px] text-zinc-400">
                  {c}
                </span>
              ))}
            </div>
          )}
        </>
      ) : (
        <div className="text-[11px] text-zinc-500">объект удалён</div>
      )}
    </div>
  )
})

type GroupData = { title: string; color: string; onTitle: (t: string) => void }
type GroupNodeType = Node<GroupData, 'group'>

const GroupFrame = memo(function GroupFrame({ data, selected }: NodeProps<GroupNodeType>) {
  return (
    <div className="h-full w-full rounded-lg border border-dashed" style={{ borderColor: `${data.color}99`, background: `${data.color}10` }}>
      <NodeResizer isVisible={selected} minWidth={160} minHeight={100} lineClassName="!border-acid/50" handleClassName="!h-2 !w-2 !border-acid !bg-void" />
      <input
        value={data.title}
        onChange={(e) => data.onTitle(e.target.value)}
        onKeyDown={(e) => e.stopPropagation()}
        placeholder="Группа"
        className="nodrag m-1.5 w-[60%] bg-transparent font-mono text-[10px] uppercase tracking-widest placeholder:text-zinc-600 focus:outline-none"
        style={{ color: data.color }}
      />
    </div>
  )
})

// 'frame' rather than 'group': xyflow ships its own styled `group` node type
const nodeTypes = { text: TextCard, object: ObjectCard, frame: GroupFrame }
const GROUP_COLORS = ['#CCFF00', '#00F0FF', '#FF0055', '#fbbf24', '#c084fc', '#86efac']

// ── board ───────────────────────────────────────────────────────────────────

function BoardInner({ node, nodes, edges: libEdges, types, disabled, onSave, onOpen, onCreate, className, full, setFull }: Props & { full: boolean; setFull: (v: boolean) => void }) {
  const rf = useReactFlow()
  const [board, setBoard] = useState<Board>(() => readBoard(node.props.board))
  const [picker, setPicker] = useState<string | null>(null)
  const loadedFor = useRef<string>('')
  const lastSaved = useRef<string>(JSON.stringify(readBoard(node.props.board)))
  const timer = useRef<number | null>(null)

  // (Re)load when another canvas is opened or the board changed remotely (agent, other window).
  useEffect(() => {
    const incoming = readBoard(node.props.board)
    const json = JSON.stringify(incoming)
    if (loadedFor.current !== node.id || (json !== lastSaved.current && timer.current == null)) {
      loadedFor.current = node.id
      lastSaved.current = json
      setBoard(incoming)
    }
  }, [node.id, node.props.board])

  const commit = useCallback(
    (next: Board | ((b: Board) => Board)) => {
      setBoard((prev) => {
        const b = typeof next === 'function' ? next(prev) : next
        if (timer.current) window.clearTimeout(timer.current)
        timer.current = window.setTimeout(() => {
          timer.current = null
          const json = JSON.stringify(b)
          if (json !== lastSaved.current) {
            lastSaved.current = json
            onSave(b)
          }
        }, 500)
        return b
      })
    },
    [onSave],
  )

  const setText = useCallback((id: string, text: string) => commit((b) => ({ ...b, items: b.items.map((i) => (i.id === id ? { ...i, text } : i)) })), [commit])
  const createFromLink = useCallback((title: string) => void onCreate(title, { type_id: 'note' }), [onCreate])

  // xyflow owns the live interaction state (drag, selection, measuring); the board only
  // receives finished gestures (drop, resize end, delete, connect), so nothing loops.
  const [rfNodes, setRfNodes, onNodesChangeBase] = useNodesState<Node>([])
  const [rfEdges, setRfEdges, onEdgesChangeBase] = useEdgesState<Edge>([])

  useEffect(() => {
    setRfNodes((prev) => {
      const keep = new Map(prev.map((n) => [n.id, n]))
      return board.items.map((it) => {
        const old = keep.get(it.id)
        const base = { id: it.id, position: { x: it.x, y: it.y }, draggable: !disabled, selectable: true, selected: old?.selected ?? false, measured: old?.measured }
        if (it.kind === 'group') {
          return { ...base, type: 'frame', zIndex: -1, style: { width: it.w ?? 320, height: it.h ?? 200 }, data: { title: it.text ?? '', color: it.color ?? GROUP_COLORS[0], onTitle: (t: string) => setText(it.id, t) } } as Node
        }
        if (it.kind === 'object') {
          const obj = nodes.find((n) => n.id === it.ref) ?? null
          return { ...base, type: 'object', data: { obj, type: obj ? types.find((t) => t.id === obj.type_id) : undefined, onOpen } } as Node
        }
        return { ...base, type: 'text', style: { width: it.w ?? 240, height: it.h }, data: { text: it.text ?? '', onText: (t: string) => setText(it.id, t), nodes, types, onOpen, onCreate: createFromLink } } as Node
      })
    })
  }, [board.items, nodes, types, disabled, onOpen, setText, createFromLink, setRfNodes])

  useEffect(() => {
    setRfEdges((prev) => {
      const sel = new Set(prev.filter((e) => e.selected).map((e) => e.id))
      return board.edges.map((e) => ({ id: e.id, source: e.from, target: e.to, label: e.label, type: 'smoothstep', selected: sel.has(e.id), markerEnd: { type: MarkerType.ArrowClosed, color: '#4a4c5c' }, labelStyle: { fill: '#9a9ca8', fontSize: 10 }, labelBgStyle: { fill: '#0D0E12' } }))
    })
  }, [board.edges, setRfEdges])

  const onNodesChange = useCallback(
    (changes: NodeChange[]) => {
      onNodesChangeBase(changes)
      const moved = changes.filter((c): c is Extract<NodeChange, { type: 'position' }> => c.type === 'position' && c.dragging === false && !!c.position)
      const resized = changes.filter((c): c is Extract<NodeChange, { type: 'dimensions' }> => c.type === 'dimensions' && c.resizing === false && !!c.dimensions)
      const removed = new Set(changes.filter((c) => c.type === 'remove').map((c) => c.id))
      if (!moved.length && !resized.length && !removed.size) return
      commit((b) => ({
        items: b.items
          .filter((i) => !removed.has(i.id))
          .map((i) => {
            const m = moved.find((c) => c.id === i.id)
            const r = resized.find((c) => c.id === i.id)
            return {
              ...i,
              ...(m?.position ? { x: Math.round(m.position.x), y: Math.round(m.position.y) } : {}),
              ...(r?.dimensions && i.kind !== 'object' ? { w: Math.round(r.dimensions.width), h: Math.round(r.dimensions.height) } : {}),
            }
          }),
        edges: b.edges.filter((e) => !removed.has(e.from) && !removed.has(e.to)),
      }))
    },
    [commit, onNodesChangeBase],
  )
  const onEdgesChange = useCallback(
    (changes: EdgeChange[]) => {
      onEdgesChangeBase(changes)
      const removed = new Set(changes.filter((c) => c.type === 'remove').map((c) => c.id))
      if (removed.size) commit((b) => ({ ...b, edges: b.edges.filter((e) => !removed.has(e.id)) }))
    },
    [commit, onEdgesChangeBase],
  )
  const onConnect = useCallback(
    (c: Connection) => {
      if (!c.source || !c.target || c.source === c.target) return
      commit((b) => (b.edges.some((e) => e.from === c.source && e.to === c.target) ? b : { ...b, edges: [...b.edges, { id: uid(), from: c.source!, to: c.target! }] }))
    },
    [commit],
  )
  const onEdgeDoubleClick = useCallback(
    (_: unknown, e: Edge) => {
      const label = window.prompt('Подпись стрелки', String(e.label ?? ''))
      if (label === null) return
      commit((b) => ({ ...b, edges: b.edges.map((x) => (x.id === e.id ? { ...x, label: label.trim() || undefined } : x)) }))
    },
    [commit],
  )

  const centre = () => {
    const el = document.querySelector('.canvas-board .react-flow') as HTMLElement | null
    const w = el?.clientWidth ?? 600
    const h = el?.clientHeight ?? 400
    const p = rf.screenToFlowPosition({ x: (el?.getBoundingClientRect().left ?? 0) + w / 2, y: (el?.getBoundingClientRect().top ?? 0) + h / 2 })
    return { x: Math.round(p.x - 120 + (Math.random() - 0.5) * 40), y: Math.round(p.y - 40 + (Math.random() - 0.5) * 40) }
  }
  const addText = () => commit((b) => ({ ...b, items: [...b.items, { id: uid(), kind: 'text', text: '', ...centre(), w: 240 }] }))
  const addGroup = () => commit((b) => ({ ...b, items: [{ id: uid(), kind: 'group', text: '', color: GROUP_COLORS[b.items.filter((i) => i.kind === 'group').length % GROUP_COLORS.length], ...centre(), w: 360, h: 220 }, ...b.items] }))
  const addObject = (id: string) => {
    commit((b) => (b.items.some((i) => i.kind === 'object' && i.ref === id) ? b : { ...b, items: [...b.items, { id: uid(), kind: 'object', ref: id, ...centre() }] }))
    setPicker(null)
  }
  const addLinked = () => {
    // Pull the library's own links between objects that are on the board.
    commit((b) => {
      const byRef = new Map(b.items.filter((i) => i.kind === 'object' && i.ref).map((i) => [i.ref!, i.id]))
      const add: BoardEdge[] = []
      for (const e of libEdges) {
        const from = byRef.get(e.source_id)
        const to = byRef.get(e.target_id)
        if (from && to && !b.edges.some((x) => (x.from === from && x.to === to) || (x.from === to && x.to === from)) && !add.some((x) => x.from === from && x.to === to)) {
          add.push({ id: uid(), from, to, label: e.relation_label || e.kind })
        }
      }
      return add.length ? { ...b, edges: [...b.edges, ...add] } : b
    })
  }
  const arrange = () => {
    commit((b) => {
      const cards = b.items.filter((i) => i.kind !== 'group')
      const cols = Math.max(1, Math.ceil(Math.sqrt(cards.length)))
      const placed = new Map(cards.map((c, i) => [c.id, { x: (i % cols) * 280, y: Math.floor(i / cols) * 180 }]))
      return { ...b, items: b.items.map((i) => (placed.has(i.id) ? { ...i, ...placed.get(i.id)! } : i)) }
    })
    window.setTimeout(() => void rf.fitView({ padding: 0.2 }), 50)
  }

  const q = (picker ?? '').trim().toLowerCase()
  const onBoard = new Set(board.items.filter((i) => i.kind === 'object').map((i) => i.ref))
  const candidates = picker === null ? [] : nodes.filter((n) => n.archived === 0 && n.id !== node.id && !onBoard.has(n.id) && (!q || n.label.toLowerCase().includes(q) || n.type_id.includes(q))).slice(0, 10)

  const btn = 'flex items-center gap-1 rounded border border-line bg-void/90 px-2 py-1 font-mono text-[10px] text-zinc-300 hover:border-acid hover:text-acid disabled:opacity-40'
  return (
    <div className={`canvas-board overflow-hidden border border-line ${full ? 'fixed inset-0 z-[70]' : `relative rounded ${className ?? 'h-[420px]'}`}`}>
      <ReactFlow
        nodes={rfNodes}
        edges={rfEdges}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={disabled ? undefined : onConnect}
        onEdgeDoubleClick={onEdgeDoubleClick}
        fitView
        fitViewOptions={{ padding: 0.3 }}
        minZoom={0.2}
        maxZoom={2.5}
        deleteKeyCode={disabled ? null : ['Backspace', 'Delete']}
        connectionRadius={40}
        nodesConnectable={!disabled}
        elementsSelectable
        proOptions={{ hideAttribution: true }}
        className="bg-void"
      >
        <Background variant={BackgroundVariant.Dots} gap={22} size={1} color="#262833" />
        <Controls showInteractive={false} position="bottom-right" />
        {full && <MiniMap pannable zoomable nodeColor="#2a2c38" maskColor="rgba(13,14,18,0.75)" position="bottom-left" />}
      </ReactFlow>
      <div className="absolute left-2 top-2 z-10 flex flex-wrap items-center gap-1">
        <button onClick={addText} disabled={disabled} className={btn} title="Текстовая карточка">
          <Type size={11} /> текст
        </button>
        <div className="relative">
          <button onClick={() => setPicker((p) => (p === null ? '' : null))} disabled={disabled} className={`${btn} ${picker !== null ? 'border-acid text-acid' : ''}`} title="Объект из библиотеки">
            <Plus size={11} /> объект
          </button>
          {picker !== null && (
            <div className="absolute left-0 top-full z-20 mt-1 w-64 rounded border border-line bg-void shadow-2xl">
              <div className="flex items-center gap-1 border-b border-line px-2 py-1">
                <Search size={11} className="text-zinc-500" />
                <input
                  autoFocus
                  value={picker}
                  onChange={(e) => setPicker(e.target.value)}
                  onKeyDown={(e) => {
                    e.stopPropagation()
                    if (e.key === 'Escape') setPicker(null)
                    if (e.key === 'Enter' && candidates[0]) addObject(candidates[0].id)
                  }}
                  placeholder="поиск по библиотеке…"
                  className="w-full bg-transparent font-mono text-[11px] text-zinc-100 placeholder:text-zinc-600 focus:outline-none"
                />
              </div>
              <div className="max-h-56 overflow-y-auto">
                {candidates.map((n) => {
                  const t = types.find((x) => x.id === n.type_id)
                  return (
                    <button key={n.id} onClick={() => addObject(n.id)} className="flex w-full items-center gap-2 px-2 py-1 text-left text-[11px] text-zinc-300 hover:bg-zinc-900">
                      <span style={{ color: t?.color }}>{t?.icon ?? '◇'}</span>
                      <span className="truncate">{n.label}</span>
                      <span className="ml-auto font-mono text-[9px] text-zinc-600">{t?.name}</span>
                    </button>
                  )
                })}
                {q && (
                  <button
                    onClick={() => {
                      const title = picker!.trim()
                      setPicker(null)
                      void Promise.resolve(onCreate(title, { type_id: 'note' })).then(() => {
                        // the new object's id is its slug; pick it up on the next render
                        window.setTimeout(() => {
                          const n = nodes.find((x) => x.label === title)
                          if (n) addObject(n.id)
                        }, 400)
                      })
                    }}
                    className="flex w-full items-center gap-2 border-t border-line px-2 py-1 text-left font-mono text-[10px] text-acid hover:bg-zinc-900"
                  >
                    <Plus size={10} /> создать заметку «{picker!.trim()}»
                  </button>
                )}
                {candidates.length === 0 && !q && <div className="px-2 py-1.5 font-mono text-[10px] text-zinc-600">введи название</div>}
              </div>
            </div>
          )}
        </div>
        <button onClick={addGroup} disabled={disabled} className={btn} title="Рамка-группа">
          <Boxes size={11} /> группа
        </button>
        <button onClick={addLinked} disabled={disabled} className={btn} title="Провести стрелки там, где объекты связаны в библиотеке">
          <Link2 size={11} /> связи
        </button>
        <button onClick={arrange} disabled={disabled} className={btn} title="Расставить сеткой">
          <LayoutGrid size={11} />
        </button>
      </div>
      <div className="absolute right-2 top-2 z-10 flex items-center gap-1">
        <span className="font-mono text-[9px] text-zinc-600">{board.items.length} карточек · {board.edges.length} стрелок</span>
        <button onClick={() => setFull(!full)} className={btn} title={full ? 'Свернуть (Esc)' : 'На весь экран'}>
          {full ? <Minimize2 size={11} /> : <Maximize2 size={11} />}
        </button>
      </div>
      {board.items.length === 0 && (
        <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
          <div className="max-w-xs text-center font-mono text-[11px] leading-relaxed text-zinc-600">
            Пустая доска. Добавь текст или объекты из библиотеки, тяни за правую точку карточки, чтобы провести стрелку. Двойной клик по стрелке — подпись, Delete — удалить.
          </div>
        </div>
      )}
    </div>
  )
}

export function CanvasBoard(props: Props) {
  const [full, setFull] = useState(false)
  useEffect(() => {
    if (!full) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setFull(false)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [full])
  const inner = (
    <ReactFlowProvider>
      <BoardInner {...props} full={full} setFull={setFull} />
    </ReactFlowProvider>
  )
  if (!full) return inner
  // a fixed overlay inside the (transformed, scrolling) side panel would be clipped: portal it out
  return (
    <>
      <div className={props.className ?? 'h-[420px]'} />
      {createPortal(inner, document.body)}
    </>
  )
}
