import { useMemo } from 'react'
import { useLive, useNoteScope } from './LiveContext'
import { parseQuery, runQuery } from './query'
import { CalendarView, ChartView, KanbanView, ListView, TableView } from './LiveViews'
import { HabitBlock } from './HabitBlock'
import { ScriptBlock } from './ScriptBlock'
import { AskBlock } from './AskBlock'

export const LIVE_LANGS = ['query', 'table', 'kanban', 'calendar', 'chart', 'list', 'habit', 'js', 'javascript', 'ask']

export function isLiveLang(lang: string) {
  return LIVE_LANGS.includes(lang.split(/\s+/)[0].toLowerCase())
}

/** Dispatch a fenced block to its widget. `onReplace` swaps the block source in the note body. */
export function LiveBlock({ lang, src, onReplace }: { lang: string; src: string; onReplace?: (next: string) => void }) {
  const kind = lang.split(/\s+/)[0].toLowerCase()
  if (kind === 'habit') return <HabitBlock src={src} />
  if (kind === 'js' || kind === 'javascript') return <ScriptBlock src={src} lang={lang} />
  if (kind === 'ask') return <AskBlock src={src} onReplace={onReplace} />
  return <QueryBlock kind={kind} src={src} />
}

function QueryBlock({ kind, src }: { kind: string; src: string }) {
  const live = useLive()
  const { nodeId } = useNoteScope()
  const q = useMemo(() => {
    // the first word of the first line is the block kind ("query", "table", …), not a search word
    const parsed = parseQuery(src.replace(/^[A-Za-z]+[ \t]*/, ''))
    if (kind !== 'query' && kind !== 'list') parsed.view = kind
    if (kind === 'list') parsed.view = 'list'
    return parsed
  }, [src, kind])
  const items = useMemo(() => (live ? runQuery(q, live.nodes, live.edges, { me: 'user', currentNode: nodeId, ...({ attachments: live.attachments } as object) }) : []), [live, q, nodeId])
  if (!live) return <pre className="md-pre">{src}</pre>
  const view = q.view
  return (
    <div className="my-2 rounded border border-line bg-void/40">
      <div className="flex items-center gap-2 border-b border-line/60 px-2 py-1 font-mono text-[9px] uppercase tracking-wider text-zinc-600">
        <span className="text-acid">{view}</span>
        <span className="truncate normal-case tracking-normal">{q.raw || 'все объекты'}</span>
        <span className="ml-auto shrink-0">{items.length}</span>
      </div>
      <div className="px-2 py-1.5">
        {items.length === 0 ? (
          <div className="font-mono text-[10px] text-zinc-600">пусто</div>
        ) : view === 'table' ? (
          <TableView items={items} types={live.types} q={q} />
        ) : view === 'kanban' ? (
          <KanbanView items={items} types={live.types} q={q} />
        ) : view === 'calendar' ? (
          <CalendarView items={items} types={live.types} q={q} />
        ) : view === 'chart' ? (
          <ChartView items={items} q={q} />
        ) : (
          <ListView items={items} types={live.types} />
        )}
      </div>
    </div>
  )
}

/** `[[button: Подпись | действие]]` — run an action from inside a note. */
export function LiveButton({ label, action, nodeId }: { label: string; action: string; nodeId: string | null }) {
  const live = useLive()
  if (!live) return <span className="font-mono text-[10px] text-zinc-600">[{label}]</span>
  const run = async () => {
    const [verb, ...rest] = action.split(':')
    const arg = rest.join(':').trim()
    const a = live.actions
    try {
      switch (verb.trim().toLowerCase()) {
        case 'tend':
          await a.tend(arg || nodeId || undefined)
          break
        case 'audit':
          await a.audit()
          break
        case 'chronicle':
          await a.chronicle()
          break
        case 'snapshot':
          await a.snapshot()
          break
        case 'tick':
          a.tick()
          break
        case 'rule': {
          const id = arg || nodeId
          if (!id) {
            a.notify('укажи правило: [[button: … | rule:<id>]]')
            break
          }
          const r = await a.runRule(id)
          a.notify(`⚙ ${r.result}`)
          break
        }
        case 'open':
          a.openNode(arg)
          break
        case 'say':
          a.say(arg, nodeId)
          break
        case 'ask': {
          const [agent, ...txt] = arg.split(':')
          a.askAgent(agent.trim(), txt.join(':').trim(), nodeId)
          break
        }
        case 'dm':
          a.dm(arg)
          break
        case 'create': {
          const [type, ...title] = arg.split(':')
          await a.createNode(title.join(':').trim() || 'Новый объект', { type_id: type.trim() })
          break
        }
        case 'timer': {
          const min = Math.max(1, Math.min(180, Number(arg) || 25))
          a.notify(`Таймер на ${min} мин запущен`)
          window.setTimeout(() => {
            a.notify(`⏰ ${min} мин прошли`)
            a.say(`*таймер на ${min} минут вышел*`, nodeId)
          }, min * 60_000)
          break
        }
        default:
          a.notify(`неизвестное действие: ${verb}`)
      }
    } catch (e) {
      a.notify(`ошибка: ${(e as Error).message}`)
    }
  }
  return (
    <button onClick={() => void run()} className="mx-0.5 inline-flex items-center gap-1 rounded border border-acid/60 bg-acid/10 px-2 py-0.5 align-baseline font-mono text-[11px] text-acid hover:bg-acid/20" title={action}>
      ▶ {label}
    </button>
  )
}
