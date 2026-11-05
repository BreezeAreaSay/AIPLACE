import { useLive, useNoteScope } from './LiveContext'

/**
 * ```habit Зарядка
 * Daily checkbox strip stored on the note as props.habit_<slug> = ["YYYY-MM-DD", …].
 */
export function HabitBlock({ src }: { src: string }) {
  const live = useLive()
  const { nodeId } = useNoteScope()
  const name = src.split('\n')[0].replace(/^habit\s*/i, '').trim() || 'привычка'
  const key = `habit_${name.toLowerCase().replace(/[^a-zа-яё0-9]+/gi, '_').replace(/^_|_$/g, '')}`
  const node = live?.nodes.find((n) => n.id === nodeId)
  const done = new Set<string>(Array.isArray(node?.props[key]) ? (node!.props[key] as unknown[]).map(String) : [])
  const days = Array.from({ length: 21 }, (_, i) => {
    const d = new Date()
    d.setDate(d.getDate() - (20 - i))
    return d
  })
  const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
  let streak = 0
  for (let i = 0; i < 400; i++) {
    const d = new Date()
    d.setDate(d.getDate() - i)
    if (done.has(iso(d))) streak++
    else if (i > 0) break
  }
  const toggle = (k: string) => {
    if (!node || !live) return
    const next = new Set(done)
    if (next.has(k)) next.delete(k)
    else next.add(k)
    live.actions.patchNode(node.id, { props: { [key]: [...next].sort() } })
  }
  if (!node) return <div className="font-mono text-[10px] text-zinc-600">привычка живёт только внутри заметки</div>
  return (
    <div className="rounded border border-line bg-panel/40 px-2 py-1.5">
      <div className="mb-1 flex items-baseline gap-2">
        <span className="text-[12px] text-zinc-200">{name}</span>
        <span className="font-mono text-[10px] text-zinc-500">
          серия {streak} {streak === 1 ? 'день' : streak < 5 ? 'дня' : 'дней'} · за 21 день: {days.filter((d) => done.has(iso(d))).length}
        </span>
      </div>
      <div className="flex flex-wrap gap-1">
        {days.map((d) => {
          const k = iso(d)
          const on = done.has(k)
          const today = iso(new Date()) === k
          return (
            <button key={k} onClick={() => toggle(k)} title={d.toLocaleDateString('ru-RU', { weekday: 'short', day: '2-digit', month: '2-digit' })} className={`h-5 w-5 rounded-sm border text-[9px] ${on ? 'border-acid bg-acid/70 text-void' : 'border-line bg-void text-zinc-600 hover:border-zinc-500'} ${today ? 'ring-1 ring-acid' : ''}`}>
              {d.getDate()}
            </button>
          )
        })}
      </div>
    </div>
  )
}
