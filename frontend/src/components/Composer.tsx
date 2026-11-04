import { useState } from 'react'
import { Dices, Send, Workflow } from 'lucide-react'
import { haptic } from '../telegram'

interface Props {
  disabled: boolean
  onSend: (text: string) => void
  onTick: () => void
  onSchema: () => void
  showEgo: boolean
  onToggleEgo: () => void
}

export function Composer({ disabled, onSend, onTick, onSchema, showEgo, onToggleEgo }: Props) {
  const [text, setText] = useState('')
  const submit = () => {
    const t = text.trim()
    if (!t || disabled) return
    haptic('light')
    onSend(t)
    setText('')
  }
  return (
    <div className="border-t border-line bg-void px-3 pb-3 pt-2" style={{ paddingBottom: 'calc(0.75rem + var(--tg-safe-bottom))' }}>
      <div className="mb-2 flex gap-2">
        <button
          onClick={() => {
            haptic('medium')
            onTick()
          }}
          disabled={disabled}
          className="flex flex-1 items-center justify-center gap-1.5 rounded border border-acid/50 px-2 py-1.5 text-xs font-mono text-acid transition hover:bg-acid/10 disabled:opacity-40"
          title="Агенты говорят между собой без тебя: спорят, мутируют граф, меняют повестки"
        >
          <Dices size={14} /> 🎲 Тик
        </button>
        <button
          onClick={() => {
            haptic('medium')
            onSchema()
          }}
          disabled={disabled}
          className="flex flex-1 items-center justify-center gap-1.5 rounded border border-cyan/50 px-2 py-1.5 text-xs font-mono text-cyan transition hover:bg-cyan/10 disabled:opacity-40"
          title="Триада собирает структурированную блок-схему темы со своими аннотациями"
        >
          <Workflow size={14} /> ⚡ Схема
        </button>
        <button
          onClick={onToggleEgo}
          className={`rounded border px-2 py-1.5 font-mono text-[10px] transition ${
            showEgo ? 'border-zinc-500 text-zinc-200' : 'border-line text-zinc-600'
          }`}
          title="Показывать скрытые мысли <ego_thought>"
        >
          ego
        </button>
      </div>
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
          placeholder="Скажи что-нибудь в комнату… (Enter — отправить)"
          className="max-h-32 min-h-[40px] flex-1 resize-y rounded border border-line bg-panel px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-600 focus:border-acid focus:outline-none"
        />
        <button
          onClick={submit}
          disabled={disabled || !text.trim()}
          className="flex h-10 w-10 items-center justify-center rounded bg-acid text-void transition hover:shadow-glow disabled:opacity-40"
          aria-label="Отправить"
        >
          <Send size={16} />
        </button>
      </div>
    </div>
  )
}
