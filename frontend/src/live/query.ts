import type { GraphEdge, GraphNode } from '../types'

/**
 * Tiny query language for live blocks (Dataview-like, but typed):
 *   type:task done:false due<7d tag:дом sort:due limit:20 view:kanban group:status
 *   text:"лампа"  owner:me  owner:physicist  linked:oscillograf_s1_94  has:file  archived:true
 *   created<7d updated>30d  status:"в работе"  rating>=4  priority:высокий
 * Free words search label/body. Dates: today, 7d, 2026-12-01.
 */
export interface Query {
  type?: string
  filters: { key: string; op: string; value: string }[]
  words: string[]
  sort?: { key: string; desc: boolean }
  limit: number
  view: string
  group?: string
  cols?: string[]
  x?: string
  y?: string
  raw: string
}

const OPS = ['>=', '<=', '!=', ':', '>', '<']

export function parseQuery(src: string): Query {
  const line = src.split('\n')[0] ?? ''
  const q: Query = { filters: [], words: [], limit: 50, view: 'list', raw: line.trim() }
  const tokens = line.match(/(?:[^\s"]+|"[^"]*")+/g) ?? []
  for (const t of tokens) {
    let op = ''
    let idx = -1
    for (const o of OPS) {
      const i = t.indexOf(o)
      if (i > 0 && (idx === -1 || i < idx)) {
        idx = i
        op = o
      }
    }
    if (idx === -1) {
      q.words.push(t.replace(/"/g, '').toLowerCase())
      continue
    }
    const key = t.slice(0, idx).toLowerCase()
    const value = t.slice(idx + op.length).replace(/^"|"$/g, '')
    switch (key) {
      case 'type':
        q.type = value
        break
      case 'sort':
        q.sort = { key: value.replace(/^-/, ''), desc: value.startsWith('-') }
        break
      case 'limit':
        q.limit = Math.max(1, Math.min(500, Number(value) || 50))
        break
      case 'view':
        q.view = value
        break
      case 'group':
        q.group = value
        break
      case 'cols':
        q.cols = value.split(',').map((s) => s.trim()).filter(Boolean)
        break
      case 'x':
        q.x = value
        break
      case 'y':
        q.y = value
        break
      case 'text':
        q.words.push(...value.toLowerCase().split(/\s+/))
        break
      default:
        q.filters.push({ key, op, value })
    }
  }
  return q
}

function parseDate(v: string): Date | null {
  const s = v.trim().toLowerCase()
  const now = new Date()
  if (s === 'today' || s === 'сегодня') return new Date(now.getFullYear(), now.getMonth(), now.getDate())
  if (s === 'now') return now
  const m = s.match(/^(-?\d+)([dwmy])$/)
  if (m) {
    const n = Number(m[1])
    const d = new Date(now)
    if (m[2] === 'd') d.setDate(d.getDate() + n)
    if (m[2] === 'w') d.setDate(d.getDate() + n * 7)
    if (m[2] === 'm') d.setMonth(d.getMonth() + n)
    if (m[2] === 'y') d.setFullYear(d.getFullYear() + n)
    return d
  }
  const d = new Date(s)
  return isNaN(d.getTime()) ? null : d
}

function cmp(a: unknown, op: string, bRaw: string, dateish: boolean): boolean {
  if (op === ':') {
    if (a === undefined || a === null) return bRaw === 'null' || bRaw === 'none' || bRaw === ''
    if (typeof a === 'boolean') return String(a) === bRaw || (bRaw === 'да' && a) || (bRaw === 'нет' && !a)
    if (Array.isArray(a)) return a.map(String).map((x) => x.toLowerCase()).includes(bRaw.toLowerCase())
    return String(a).toLowerCase() === bRaw.toLowerCase() || String(a).toLowerCase().includes(bRaw.toLowerCase())
  }
  if (op === '!=') return !cmp(a, ':', bRaw, dateish)
  if (a === undefined || a === null || a === '') return false
  let av: number
  let bv: number
  if (dateish || /^\d{4}-\d{2}-\d{2}/.test(String(a))) {
    const ad = new Date(String(a))
    const bd = parseDate(bRaw)
    if (isNaN(ad.getTime()) || !bd) return false
    av = ad.getTime()
    bv = bd.getTime()
  } else {
    av = Number(a)
    bv = Number(bRaw)
    if (isNaN(av) || isNaN(bv)) return false
  }
  if (op === '>') return av > bv
  if (op === '<') return av < bv
  if (op === '>=') return av >= bv
  if (op === '<=') return av <= bv
  return false
}

const SPECIAL_DUE_WORDS: Record<string, (n: GraphNode) => boolean> = {
  overdue: (n) => {
    const d = n.props.due ?? n.props.date
    return typeof d === 'string' && d.slice(0, 10) < new Date().toISOString().slice(0, 10) && n.props.done !== true
  },
}

export function runQuery(q: Query, nodes: GraphNode[], edges: GraphEdge[], ctx: { me?: string; currentNode?: string | null }): GraphNode[] {
  let list = nodes.filter((n) => n.archived === 0 || q.filters.some((f) => f.key === 'archived'))
  if (q.type) list = list.filter((n) => n.type_id === q.type || n.type_id.split(',').includes(q.type!))
  for (const f of q.filters) {
    list = list.filter((n) => {
      switch (f.key) {
        case 'tag':
          return (Array.isArray(n.props.tags) ? (n.props.tags as unknown[]).map(String) : []).some((t) => t.toLowerCase() === f.value.toLowerCase())
        case 'owner':
        case 'author':
        case 'by':
          return n.created_by === (f.value === 'me' ? 'user' : f.value)
        case 'archived':
          return (n.archived === 1) === (f.value === 'true')
        case 'has':
          if (f.value === 'file' || f.value === 'files') return (ctx as { attachments?: { node_id: string }[] }).attachments?.some((a) => a.node_id === n.id) ?? false
          if (f.value === 'body') return n.body.trim().length > 0
          if (f.value === 'links') return edges.some((e) => e.source_id === n.id || e.target_id === n.id)
          return n.props[f.value] !== undefined && n.props[f.value] !== null && n.props[f.value] !== ''
        case 'linked': {
          const target = f.value === 'this' ? ctx.currentNode : f.value
          if (!target) return false
          return edges.some((e) => (e.source_id === n.id && e.target_id === target) || (e.target_id === n.id && e.source_id === target))
        }
        case 'id':
          return n.id === f.value
        case 'created':
          return cmp(n.created_at, f.op, f.value, true)
        case 'updated':
          return cmp(n.updated_at, f.op, f.value, true)
        case 'due':
        case 'date':
          if (f.op === ':' && SPECIAL_DUE_WORDS[f.value]) return SPECIAL_DUE_WORDS[f.value](n)
          return cmp(n.props[f.key] ?? n.props.due ?? n.props.date, f.op, f.value, true)
        case 'label':
        case 'title':
          return n.label.toLowerCase().includes(f.value.toLowerCase())
        default:
          return cmp(n.props[f.key], f.op, f.value, false)
      }
    })
  }
  if (q.words.length) {
    list = list.filter((n) => {
      const hay = `${n.label} ${n.body} ${JSON.stringify(n.props)}`.toLowerCase()
      return q.words.every((w) => hay.includes(w))
    })
  }
  const sortKey = q.sort?.key ?? 'updated'
  const desc = q.sort ? q.sort.desc : true
  list.sort((a, b) => {
    const av = sortKey === 'updated' ? a.updated_at : sortKey === 'created' ? a.created_at : sortKey === 'label' ? a.label : (a.props[sortKey] as string | number | undefined) ?? ''
    const bv = sortKey === 'updated' ? b.updated_at : sortKey === 'created' ? b.created_at : sortKey === 'label' ? b.label : (b.props[sortKey] as string | number | undefined) ?? ''
    const r = typeof av === 'number' && typeof bv === 'number' ? av - bv : String(av).localeCompare(String(bv), 'ru')
    return desc ? -r : r
  })
  return list.slice(0, q.limit)
}
