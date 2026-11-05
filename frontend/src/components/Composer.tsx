import { useMemo, useRef, useState } from 'react'
import { BookmarkPlus, Dices, Mic, Paperclip, Send, Square, X } from 'lucide-react'
import { haptic } from '../telegram'
import type { Features, GraphNode, ObjectType } from '../types'
import { api } from '../api'

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
  features: Features
  /** Upload files as objects; returns their titles for [[links]]. */
  onAttach: (files: File[]) => Promise<string[]>
}

type SR = { start: () => void; stop: () => void; lang: string; interimResults: boolean; continuous: boolean; onresult: ((e: { results: ArrayLike<ArrayLike<{ transcript: string }>> }) => void) | null; onend: (() => void) | null; onerror: (() => void) | null }
const BrowserSR = (window as unknown as { SpeechRecognition?: new () => SR; webkitSpeechRecognition?: new () => SR }).SpeechRecognition ?? (window as unknown as { webkitSpeechRecognition?: new () => SR }).webkitSpeechRecognition

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

export function Composer({ disabled, types, onSend, onTick, showEgo, onToggleEgo, contextNode, onClearContext, onCapture, onSearch, features, onAttach }: Props) {
  const fileInput = useRef<HTMLInputElement>(null)
  const [rec, setRec] = useState<'off' | 'rec' | 'busy'>('off')
  const recorder = useRef<MediaRecorder | null>(null)
  const sr = useRef<SR | null>(null)
  const canVoice = features.stt || !!BrowserSR

  const attach = async (files: File[]) => {
    if (!files.length) return
    say(`гружу ${files.length}…`)
    try {
      const titles = await onAttach(files)
      setText((t) => `${t}${t && !t.endsWith(' ') ? ' ' : ''}${titles.map((x) => `[[${x}]]`).join(' ')} `)
      say(`в библиотеке: ${titles.join(', ')}`)
    } catch (e) {
      say(`ошибка: ${(e as Error).message}`)
    }
  }

  const stopVoice = () => {
    recorder.current?.stop()
    sr.current?.stop()
  }
  const startVoice = async () => {
    if (features.stt && navigator.mediaDevices?.getUserMedia) {
      try {
        const stream = await navigator.mediaDevices.getUserMedia({ audio: true })
        const mime = MediaRecorder.isTypeSupported('audio/webm;codecs=opus') ? 'audio/webm;codecs=opus' : 'audio/webm'
        const mr = new MediaRecorder(stream, { mimeType: mime })
        const chunks: Blob[] = []
        mr.ondataavailable = (e) => e.data.size && chunks.push(e.data)
        mr.onstop = async () => {
          stream.getTracks().forEach((t) => t.stop())
          setRec('busy')
          try {
            const r = await api.stt(new Blob(chunks, { type: mime }))
            setText((t) => (t ? `${t} ${r.text}` : r.text))
          } catch (e) {
            say(`распознавание: ${(e as Error).message}`)
          } finally {
            setRec('off')
          }
        }
        recorder.current = mr
        mr.start()
        setRec('rec')
        return
      } catch (e) {
        say(`микрофон: ${(e as Error).message}`)
      }
    }
    if (BrowserSR) {
      const r = new BrowserSR()
      r.lang = 'ru-RU'
      r.interimResults = false
      r.continuous = true
      let acc = ''
      r.onresult = (e) => {
        acc = Array.from({ length: e.results.length }, (_, i) => e.results[i][0].transcript).join(' ')
        setText((t) => `${t.replace(/ \[голос\].*$/, '')} [голос] ${acc}`.trim())
      }
      r.onend = () => {
        setText((t) => t.replace(' [голос] ', ' ').trim())
        setRec('off')
      }
      r.onerror = () => setRec('off')
      sr.current = r
      r.start()
      setRec('rec')
    }
  }
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
          onPaste={(e) => {
            const files = Array.from(e.clipboardData.files)
            if (files.length) {
              e.preventDefault()
              void attach(files)
            }
          }}
          onDragOver={(e) => {
            if (e.dataTransfer.types.includes('Files')) e.preventDefault()
          }}
          onDrop={(e) => {
            if (e.dataTransfer.files.length) {
              e.preventDefault()
              void attach(Array.from(e.dataTransfer.files))
            }
          }}
          rows={1}
          placeholder={disabled ? 'Они отвечают — можно докинуть ещё, подхватят по ходу…' : 'Скажи что-нибудь в комнату… («/» — команды, Shift+Enter — перенос)'}
          className="max-h-40 min-h-[42px] flex-1 resize-y rounded border border-line bg-panel px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-600 focus:border-acid focus:outline-none"
        />
        <input
          ref={fileInput}
          type="file"
          multiple
          className="hidden"
          onChange={(e) => {
            if (e.target.files) void attach(Array.from(e.target.files))
            e.target.value = ''
          }}
        />
        <button onClick={() => fileInput.current?.click()} className="flex h-10 w-10 items-center justify-center rounded border border-line text-zinc-400 transition hover:border-acid hover:text-acid" title="Приложить файлы: станут объектами библиотеки и [[ссылками]] в сообщении">
          <Paperclip size={16} />
        </button>
        {canVoice && (
          <button
            onClick={() => (rec === 'rec' ? stopVoice() : rec === 'off' ? void startVoice() : undefined)}
            className={`flex h-10 w-10 items-center justify-center rounded border transition ${rec === 'rec' ? 'animate-pulse border-magenta text-magenta' : 'border-line text-zinc-400 hover:border-acid hover:text-acid'}`}
            title={rec === 'rec' ? 'Стоп' : features.stt ? 'Надиктовать (Whisper)' : 'Надиктовать (распознавание браузера)'}
          >
            {rec === 'rec' ? <Square size={14} /> : <Mic size={16} className={rec === 'busy' ? 'animate-pulse' : ''} />}
          </button>
        )}
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
