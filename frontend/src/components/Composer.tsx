import { useMemo, useState } from 'react'
import { BookmarkPlus, Dices, Send, X } from 'lucide-react'
import { haptic } from '../telegram'
import type { GraphNode, ObjectType } from '../types'

interface Props {
  disabled: boolean
  types: ObjectType[]
  onSend: (text: string, nodeId?: string | null) => void
  onTick: () => void
  showEgo: boolean
  onToggleEgo: () => void
  contextNode: GraphNode | null
  onClearContext: () => void
  /** Save straight into the library without talking to the room. */
  onCapture: (label: string, typeId: string, body: string) => void
  onSearch: (q: string) => void
}

interface Command {
  names: string[]
  type?: string
  hint: string
  arg: string
}

const COMMANDS: Command[] = [
  { names: ['заметка', 'note', 'n'], type: 'note', hint: 'заметка в библиотеку (первая строка — заголовок)', arg: 'текст' },
  { names: ['задача', 'task', 't'], type: 'task', hint: 'задача себе', arg: 'что сделать' },
  { names: ['идея', 'idea', 'i'], type: 'idea', hint: 'идея', arg: 'суть' },
  { names: ['человек', 'human', 'h'], type: 'human', hint: 'человек', arg: 'имя' },
  { names: ['цель', 'goal', 'g'], type: 'goal', hint: 'цель', arg: 'формулировка' },
  { names: ['дневник', 'diary', 'd'], type: 'diary', hint: 'запись дневника за сегодня', arg: 'текст' },
  { names: ['найти', 'find', 'f', 's'], hint: 'поиск по библиотеке', arg: 'слова' },
  { names: ['спор', 'tick'], hint: 'пусть поспорят сами', arg: '' },
]

function parseCommand(text: string): { cmd: Command; arg: string } | null {
  const m = text.match(/^\/([a-zа-яё]+)\s*([\s\S]*)$/i)
  if (!m) return null
  const cmd = COMMANDS.find((c) => c.names.includes(m[1].toLowerCase()))
  return cmd ? { cmd, arg: m[2].trim() } : null
}

export function Composer({ disabled, types, onSend, onTick, showEgo, onToggleEgo, contextNode, onClearContext, onCapture, onSearch }: Props) {
  const [text, setText] = useState('')
  const [flash, setFlash] = useState<string | null>(null)
  const slash = text.startsWith('/')
  const parsed = useMemo(() => (slash ? parseCommand(text) : null), [slash, text])
  const say = (s: string) => {
    setFlash(s)
    window.setTimeout(() => setFlash(null), 1800)
  }

  const capture = (typeId: string, raw: string) => {
    const [first, ...rest] = raw.split('\n')
    const label = first.trim().slice(0, 120)
    if (!label) return
    onCapture(label, typeId, rest.join('\n').trim())
    const t = types.find((x) => x.id === typeId)
    say(`${t?.icon ?? '◇'} в библиотеке: ${label}`)
  }

  const submit = () => {
    const t = text.trim()
    if (!t) return
    haptic('light')
    if (parsed) {
      const { cmd, arg } = parsed
      if (cmd.type) {
        if (!arg) return
        capture(cmd.type, arg)
      } else if (cmd.names[0] === 'найти') {
        if (!arg) return
        onSearch(arg)
      } else if (cmd.names[0] === 'спор') {
        onTick()
      }
      setText('')
      return
    }
    onSend(t, contextNode?.id ?? null)
    setText('')
  }

  return (
    <div className="border-t border-line bg-void px-3 pb-3 pt-2" style={{ paddingBottom: 'calc(0.75rem + var(--tg-safe-bottom))' }}>
      {contextNode && (
        <div className="mb-2 flex items-center gap-2 rounded border border-line bg-panel px-2 py-1 font-mono text-[10px] text-zinc-400">
          <span>говорим про объект</span>
          <span className="truncate text-zinc-200">◇ {contextNode.label}</span>
          <button onClick={onClearContext} className="ml-auto text-zinc-500 hover:text-zinc-200" aria-label="Сбросить контекст">
            <X size={12} />
          </button>
        </div>
      )}
      {slash && (
        <div className="mb-2 rounded border border-line bg-panel px-2 py-1.5 font-mono text-[10px] text-zinc-400">
          {parsed ? (
            <span>
              <span className="text-acid">/{parsed.cmd.names[0]}</span> {parsed.cmd.hint}
              {parsed.cmd.arg && !parsed.arg && <span className="text-zinc-600"> · {parsed.cmd.arg}…</span>}
            </span>
          ) : (
            <div className="flex flex-wrap gap-x-3 gap-y-0.5">
              {COMMANDS.map((c) => (
                <button key={c.names[0]} onClick={() => setText(`/${c.names[0]} `)} className="hover:text-acid">
                  <span className="text-zinc-200">/{c.names[0]}</span> {c.hint}
                </button>
              ))}
            </div>
          )}
        </div>
      )}
      {flash && <div className="mb-2 font-mono text-[10px] text-acid">{flash}</div>}
      <div className="flex items-end gap-2">
        <textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault()
              submit()
            }
          }}
          rows={1}
          placeholder={disabled ? 'Они отвечают — можно докинуть ещё, подхватят по ходу…' : 'Скажи что-нибудь в комнату… («/» — команды, Shift+Enter — перенос)'}
          className="max-h-40 min-h-[42px] flex-1 resize-y rounded border border-line bg-panel px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-600 focus:border-acid focus:outline-none"
        />
        <button
          onClick={() => {
            if (text.trim()) capture('note', text.trim())
            setText('')
          }}
          disabled={!text.trim() || slash}
          className="flex h-10 w-10 items-center justify-center rounded border border-line text-zinc-400 transition hover:border-acid hover:text-acid disabled:opacity-30"
          title="Не говорить — сохранить как заметку в библиотеку"
        >
          <BookmarkPlus size={16} />
        </button>
        <button onClick={submit} disabled={!text.trim()} className="flex h-10 w-10 items-center justify-center rounded bg-acid text-void transition hover:shadow-glow disabled:opacity-40" aria-label="Отправить">
          <Send size={16} />
        </button>
      </div>
      <div className="mt-2 flex items-center gap-2">
        <button
          onClick={() => {
            haptic('medium')
            onTick()
          }}
          disabled={disabled}
          className="flex items-center gap-1.5 rounded border border-acid/50 px-2 py-1 font-mono text-[11px] text-acid transition hover:bg-acid/10 disabled:opacity-40"
          title="Агенты говорят между собой без тебя"
        >
          <Dices size={13} /> пусть поспорят сами
        </button>
        <button
          onClick={onToggleEgo}
          className={`ml-auto rounded border px-2 py-1 font-mono text-[10px] transition ${showEgo ? 'border-zinc-500 text-zinc-200' : 'border-line text-zinc-600'}`}
          title="Показывать скрытые мысли"
        >
          мысли {showEgo ? 'вкл' : 'выкл'}
        </button>
      </div>
    </div>
  )
}
