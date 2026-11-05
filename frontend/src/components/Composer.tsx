import { useState } from 'react'
import { Dices, Send, X } from 'lucide-react'
import { haptic } from '../telegram'
import type { GraphNode } from '../types'

interface Props {
  disabled: boolean
  onSend: (text: string, nodeId?: string | null) => void
  onTick: () => void
  showEgo: boolean
  onToggleEgo: () => void
  contextNode: GraphNode | null
  onClearContext: () => void
}

export function Composer({ disabled, onSend, onTick, showEgo, onToggleEgo, contextNode, onClearContext }: Props) {
  const [text, setText] = useState('')
  const submit = () => {
    const t = text.trim()
    if (!t || disabled) return
    haptic('light')
    onSend(t, contextNode?.id ?? null)
    setText('')
  }
  return (
    <div className="border-t border-line bg-void px-3 pb-3 pt-2" style={{ paddingBottom: 'calc(0.75rem + var(--tg-safe-bottom))' }}>
      {contextNode && (
        <div className="mb-2 flex items-center gap-2 rounded border border-line bg-panel px-2 py-1 font-mono text-[10px] text-zinc-400">
          <span>говорим про узел</span>
          <span className="truncate text-zinc-200">◇ {contextNode.label}</span>
          <button onClick={onClearContext} className="ml-auto text-zinc-500 hover:text-zinc-200" aria-label="Сбросить контекст">
            <X size={12} />
          </button>
        </div>
      )}
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
          placeholder="Скажи что-нибудь в комнату… (Enter — отправить, Shift+Enter — перенос)"
          className="max-h-40 min-h-[42px] flex-1 resize-y rounded border border-line bg-panel px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-600 focus:border-acid focus:outline-none"
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
