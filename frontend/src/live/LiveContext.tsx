import { createContext, useContext, type ReactNode } from 'react'
import type { Agent, Attachment, GraphEdge, GraphNode, ObjectType } from '../types'

/** What live blocks inside a note can see and do. Provided once at the app root. */
export interface LiveActions {
  openNode: (id: string) => void
  patchNode: (id: string, patch: { body?: string; props?: Record<string, unknown>; label?: string; archived?: boolean }) => void
  createNode: (label: string, opts: { type_id?: string; body?: string; props?: Record<string, unknown> }) => Promise<unknown> | void
  say: (text: string, nodeId?: string | null) => void
  askAgent: (agentId: string, text: string, nodeId?: string | null) => void
  dm: (agentId: string) => void
  tend: (nodeId?: string) => Promise<unknown>
  audit: () => Promise<unknown>
  chronicle: () => Promise<unknown>
  snapshot: () => Promise<unknown>
  tick: () => void
  /** Run a `rule` object now (ignores its schedule). */
  runRule: (ruleId: string) => Promise<{ result: string }>
  notify: (text: string) => void
}

export interface LiveData {
  nodes: GraphNode[]
  edges: GraphEdge[]
  types: ObjectType[]
  attachments: Attachment[]
  agents: Agent[]
  actions: LiveActions
}

const Ctx = createContext<LiveData | null>(null)
const NoteCtx = createContext<{ nodeId: string | null; depth: number }>({ nodeId: null, depth: 0 })

export function LiveProvider({ value, children }: { value: LiveData; children: ReactNode }) {
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>
}

/** Which note a block lives in (for formulas, habits, embeds recursion). */
export function NoteScope({ nodeId, depth = 0, children }: { nodeId: string | null; depth?: number; children: ReactNode }) {
  return <NoteCtx.Provider value={{ nodeId, depth }}>{children}</NoteCtx.Provider>
}

export function useLive(): LiveData | null {
  return useContext(Ctx)
}

export function useNoteScope() {
  return useContext(NoteCtx)
}
