import type { GraphEdge, GraphNode } from '../types'

/**
 * Formula properties: `expr` evaluated client-side over the object's props and the library.
 *   progress * 2, days_until(due), done_ratio(), count(linked, "task"), len(tags), now()
 * Identifiers resolve to props; a whitelist keeps it to arithmetic and the helpers below.
 */
export interface FormulaEnv {
  node: GraphNode
  nodes: GraphNode[]
  edges: GraphEdge[]
}

function helpers(env: FormulaEnv) {
  const linked = () => {
    const ids = new Set(env.edges.filter((e) => e.source_id === env.node.id || e.target_id === env.node.id).map((e) => (e.source_id === env.node.id ? e.target_id : e.source_id)))
    const byProp = env.nodes.filter((n) => Object.values(n.props).includes(env.node.id)).map((n) => n.id)
    for (const id of byProp) ids.add(id)
    return env.nodes.filter((n) => ids.has(n.id) && n.archived === 0)
  }
  const dayMs = 86_400_000
  return {
    now: () => Date.now(),
    today: () => new Date().toISOString().slice(0, 10),
    days_until: (d: unknown) => (typeof d === 'string' && d ? Math.ceil((new Date(d).getTime() - Date.now()) / dayMs) : null),
    days_since: (d: unknown) => (typeof d === 'string' && d ? Math.floor((Date.now() - new Date(d).getTime()) / dayMs) : null),
    len: (v: unknown) => (Array.isArray(v) ? v.length : typeof v === 'string' ? v.length : 0),
    count: (what: string = 'linked', type?: string) => {
      const set = what === 'all' ? env.nodes.filter((n) => n.archived === 0) : linked()
      return set.filter((n) => !type || n.type_id === type).length
    },
    done_ratio: (type = 'task') => {
      const set = linked().filter((n) => n.type_id === type)
      if (!set.length) return 0
      return Math.round((set.filter((n) => n.props.done === true).length / set.length) * 100)
    },
    sum: (prop: string, type?: string) => linked().filter((n) => !type || n.type_id === type).reduce((a, n) => a + (Number(n.props[prop]) || 0), 0),
    avg: (prop: string, type?: string) => {
      const vals = linked().filter((n) => !type || n.type_id === type).map((n) => Number(n.props[prop])).filter((v) => !isNaN(v))
      return vals.length ? Math.round((vals.reduce((a, b) => a + b, 0) / vals.length) * 10) / 10 : null
    },
    min: Math.min,
    max: Math.max,
    round: (v: number, d = 0) => Math.round(v * 10 ** d) / 10 ** d,
    if: (c: unknown, a: unknown, b: unknown) => (c ? a : b),
    word_count: () => (env.node.body.trim() ? env.node.body.trim().split(/\s+/).length : 0),
  }
}

const HELPER_NAMES = ['now', 'today', 'days_until', 'days_since', 'len', 'count', 'done_ratio', 'sum', 'avg', 'min', 'max', 'round', 'if', 'word_count']

export function evalFormula(expr: string, env: FormulaEnv): { value: unknown; error?: string } {
  const src = expr.trim()
  if (!src) return { value: null }
  // whitelist: identifiers, numbers, strings, arithmetic, comparisons, parens, commas, ternary
  if (!/^[\w\s.+\-*/%()<>=!?:,"'а-яё]+$/i.test(src) || /\b(function|=>|while|for|new|this|window|document|import|eval|constructor|prototype)\b/.test(src)) {
    return { value: null, error: 'недопустимое выражение' }
  }
  const h = helpers(env)
  const p = env.node.props
  // identifiers not followed by "(" → props lookup; helper names stay
  const rewritten = src.replace(/(?<![\w."'])([a-zA-Zа-яёА-ЯЁ_][\w]*)(?!\s*\()(?![\w"'])/g, (m) => (HELPER_NAMES.includes(m) || m === 'true' || m === 'false' || m === 'null' ? m : `p[${JSON.stringify(m)}]`))
  try {
    // eslint-disable-next-line @typescript-eslint/no-implied-eval
    const fn = new Function('p', ...HELPER_NAMES, `"use strict"; return (${rewritten});`)
    const value = fn(p, ...HELPER_NAMES.map((n) => (h as Record<string, unknown>)[n]))
    return { value: typeof value === 'number' && !isFinite(value) ? null : value }
  } catch (e) {
    return { value: null, error: (e as Error).message }
  }
}

export function formatValue(v: unknown): string {
  if (v === null || v === undefined) return '—'
  if (typeof v === 'number') return Number.isInteger(v) ? String(v) : v.toFixed(1)
  if (typeof v === 'boolean') return v ? 'да' : 'нет'
  if (Array.isArray(v)) return v.join(', ')
  return String(v)
}
