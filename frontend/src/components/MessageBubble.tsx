import { useEffect, useRef, useState } from 'react'
import { Check, ChevronDown, ChevronRight, CornerUpLeft, Lock, Pencil, SmilePlus, Terminal, Trash2, Volume2, VolumeX, X } from 'lucide-react'
import type { Proposal } from '../types'
import { ProposalCard } from './ProposalCard'
import { api } from '../api'
import type { Agent, GraphNode, Message, ObjectType } from '../types'
import { AGENT_COLORS, AGENT_GLYPH } from '../types'
import { Markdown } from './Markdown'

interface Props {
  message: Message
  agents: Agent[]
  nodes: GraphNode[]
  types?: ObjectType[]
  showEgo: boolean
  onNodeClick?: (id: string) => void
  /** Server TTS configured; otherwise the browser's own voices are used. */
  serverTts?: boolean
  onEdit?: (id: number, content: string) => void
  onDelete?: (id: number) => void
  highlight?: boolean
  onReact?: (id: number, emoji: string) => void
  onReply?: (m: Message) => void
  onDm?: (agentId: string) => void
  /** For resolving reply_to quotes. */
  messages?: Message[]
  proposals?: Proposal[]
  onAcceptProposal?: (id: number) => Promise<unknown>
  onRejectProposal?: (id: number) => Promise<unknown>
}

const QUICK = ['👍', '🔥', '❤️', '😂', '🤔', '👀', '💀', '🙏']

function Reactions({ m, agents, onReact }: { m: Message; agents: Agent[]; onReact?: (id: number, emoji: string) => void }) {
  const [pick, setPick] = useState(false)
  const entries = Object.entries(m.reactions ?? {})
  const who = (id: string) => (id === 'user' ? 'ты' : agents.find((a) => a.id === id)?.name.split(' ')[0] ?? id)
  return (
    <div className="mt-1 flex flex-wrap items-center gap-1">
      {entries.map(([emoji, users]) => (
        <button
          key={emoji}
          onClick={() => onReact?.(m.id, emoji)}
          className={`flex items-center gap-1 rounded-full border px-1.5 py-0.5 text-[11px] ${users.includes('user') ? 'border-acid/60 bg-acid/10' : 'border-line bg-panel hover:border-zinc-500'}`}
          title={users.map(who).join(', ')}
        >
          <span>{emoji}</span>
          <span className="font-mono text-[9px] text-zinc-400">{users.length}</span>
        </button>
      ))}
      {onReact && (
        <span className="relative">
          <button onClick={() => setPick((v) => !v)} className={`rounded-full border border-dashed border-line p-0.5 text-zinc-600 hover:border-zinc-500 hover:text-zinc-300 ${entries.length === 0 ? 'hidden group-hover:inline-flex' : 'inline-flex'}`} title="Реакция">
            <SmilePlus size={12} />
          </button>
          {pick && (
            <span className="absolute bottom-full left-0 z-20 mb-1 flex gap-0.5 rounded border border-line bg-void px-1 py-0.5 shadow-xl">
              {QUICK.map((e) => (
                <button
                  key={e}
                  onClick={() => {
                    onReact(m.id, e)
                    setPick(false)
                  }}
                  className="rounded px-1 text-base hover:bg-zinc-800"
                >
                  {e}
                </button>
              ))}
            </span>
          )}
        </span>
      )}
    </div>
  )
}

function Quote({ id, messages, agents }: { id: number | null; messages?: Message[]; agents: Agent[] }) {
  if (!id || !messages) return null
  const q = messages.find((x) => x.id === id)
  if (!q) return null
  const who = q.sender_id === 'user' ? 'ты' : agents.find((a) => a.id === q.sender_id)?.name.split(' ')[0] ?? q.sender_id
  return (
    <button onClick={() => document.getElementById(`msg-${q.id}`)?.scrollIntoView({ block: 'center' })} className="mb-1 flex w-full items-baseline gap-1.5 overflow-hidden rounded border-l-2 bg-black/30 px-2 py-0.5 text-left text-[11px]" style={{ borderColor: AGENT_COLORS[q.sender_id] ?? '#666' }}>
      <span className="shrink-0 font-mono text-[10px]" style={{ color: AGENT_COLORS[q.sender_id] ?? '#888' }}>
        {who}
      </span>
      <span className="truncate text-zinc-400">{q.content}</span>
    </button>
  )
}

const VOICE_PREF: Record<string, { pitch: number; rate: number }> = {
  physicist: { pitch: 0.75, rate: 0.92 },
  lit_teacher: { pitch: 1.2, rate: 0.95 },
  graffiti: { pitch: 1.0, rate: 1.12 },
}

function pickVoice(agentId: string): SpeechSynthesisVoice | null {
  const ru = speechSynthesis.getVoices().filter((v) => v.lang.toLowerCase().startsWith('ru'))
  if (!ru.length) return null
  const female = ru.find((v) => /svetlana|irina|ekaterina|milena|alena|female|женск/i.test(v.name))
  const male = ru.find((v) => /pavel|dmitry|yuri|male|мужск/i.test(v.name) && !/female/i.test(v.name))
  if (agentId === 'lit_teacher') return female ?? ru[0]
  return male ?? ru[ru.length > 1 ? 1 : 0]
}

/** Strip markdown / stage directions for a cleaner read-aloud. */
function speakable(text: string) {
  return text
    .replace(/\*[^*\n]{1,200}\*/g, '')
    .replace(/\[\[([^\]|]+)(\|[^\]]+)?\]\]/g, '$1')
    .replace(/[#*_`>]/g, '')
    .replace(/\(https?:\/\/\S+\)/g, '')
    .replace(/\s+/g, ' ')
    .trim()
}

function useSpeaker(agentId: string, text: string, serverTts: boolean) {
  const [state, setState] = useState<'idle' | 'loading' | 'playing'>('idle')
  const audio = useRef<HTMLAudioElement | null>(null)
  useEffect(
    () => () => {
      audio.current?.pause()
    },
    [],
  )
  const stop = () => {
    audio.current?.pause()
    audio.current = null
    if ('speechSynthesis' in window) speechSynthesis.cancel()
    setState('idle')
  }
  const play = async () => {
    const t = speakable(text)
    if (!t) return
    if (serverTts) {
      setState('loading')
      try {
        const blob = await api.tts(t, agentId)
        const a = new Audio(URL.createObjectURL(blob))
        audio.current = a
        a.onended = () => setState('idle')
        await a.play()
        setState('playing')
        return
      } catch {
        /* fall through to the browser voice */
      }
    }
    if (!('speechSynthesis' in window)) {
      setState('idle')
      return
    }
    speechSynthesis.cancel()
    const u = new SpeechSynthesisUtterance(t)
    u.lang = 'ru-RU'
    const v = pickVoice(agentId)
    if (v) u.voice = v
    const pref = VOICE_PREF[agentId] ?? { pitch: 1, rate: 1 }
    u.pitch = pref.pitch
    u.rate = pref.rate
    u.onend = () => setState('idle')
    u.onerror = () => setState('idle')
    speechSynthesis.speak(u)
    setState('playing')
  }
  return { state, play, stop }
}

export function MessageBubble({ message, agents, nodes, types = [], showEgo, onNodeClick, serverTts = false, onEdit, onDelete, highlight, onReact, onReply, onDm, messages, proposals = [], onAcceptProposal, onRejectProposal }: Props) {
  const mine = proposals.filter((p) => p.message_id === message.id && p.status === 'pending')
  const [open, setOpen] = useState(false)
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(message.content)
  const speaker = useSpeaker(message.sender_id, message.content, serverTts)
  const canSpeak = serverTts || 'speechSynthesis' in window
  const agent = agents.find((a) => a.id === message.sender_id)
  const isUser = message.sender_id === 'user'
  const isSystem = message.sender_id === 'system'
  const color = agent?.color ?? AGENT_COLORS[message.sender_id] ?? '#aaa'
  const time = new Date(message.created_at).toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' })
  const node = message.node_id ? nodes.find((n) => n.id === message.node_id) : undefined
  const silent = /^\*[^*]+\*$/.test(message.content.trim())

  const mentioned = (message.objects ?? []).map((id) => nodes.find((n) => n.id === id)).filter((n): n is GraphNode => !!n && n.id !== node?.id)
  const chip = (n: GraphNode, strong = false) => {
    const t = types.find((x) => x.id === n.type_id)
    return (
      <button
        key={n.id}
        onClick={() => onNodeClick?.(n.id)}
        className={`inline-flex max-w-[220px] items-center gap-1 rounded border px-1.5 py-0.5 font-mono text-[10px] hover:border-acid hover:text-acid ${strong ? 'border-zinc-500 text-zinc-200' : 'border-line bg-panel text-zinc-400'}`}
        title={`${t?.name ?? ''}: открыть`}
      >
        <span style={{ color: t?.color }}>{t?.icon ?? '◇'}</span>
        <span className="truncate">{n.label}</span>
      </button>
    )
  }
  const nodeChip = (node || mentioned.length > 0) && (
    <div className="mb-1 flex flex-wrap gap-1">
      {node && chip(node, true)}
      {mentioned.map((n) => chip(n))}
    </div>
  )

  if (isSystem && message.move === 'итог') {
    // The round's artifact: the case note the yard keeps for this problem, with its objects as chips.
    return (
      <div className="my-1.5 max-w-[92%] rounded border border-acid/40 bg-acid/5 px-3 py-2">
        <div className="text-[12px] leading-snug text-zinc-200">{message.content}</div>
        <div className="mt-1.5 flex flex-wrap gap-1">
          {node && chip(node, true)}
          {mentioned.map((n) => chip(n))}
        </div>
      </div>
    )
  }
  if (isSystem) {
    return (
      <div className="mx-auto my-1 max-w-[92%] rounded border border-dashed border-line px-3 py-1.5 text-center text-[11px] font-mono text-zinc-500">
        {message.content}
      </div>
    )
  }

  if (isUser) {
    return (
      <div className="group flex flex-col items-end">
        {nodeChip}
        <div className={`max-w-[90%] rounded-lg rounded-br-sm border bg-zinc-800/70 px-3 py-2 text-sm text-zinc-100 ${highlight ? 'border-acid shadow-glow' : 'border-zinc-600'}`}>
          <Quote id={message.reply_to} messages={messages} agents={agents} />
          {editing ? (
            <div className="min-w-[260px]">
              <textarea value={draft} onChange={(e) => setDraft(e.target.value)} rows={Math.min(8, draft.split('\n').length + 1)} className="w-full resize-y rounded border border-line bg-void px-2 py-1 text-sm text-zinc-100 focus:border-acid focus:outline-none" autoFocus />
              <div className="mt-1 flex justify-end gap-1">
                <button onClick={() => setEditing(false)} className="rounded border border-line px-2 py-0.5 font-mono text-[10px] text-zinc-400">
                  <X size={10} className="inline" /> отмена
                </button>
                <button
                  onClick={() => {
                    if (draft.trim() && draft.trim() !== message.content) onEdit?.(message.id, draft.trim())
                    setEditing(false)
                  }}
                  className="rounded bg-acid px-2 py-0.5 font-mono text-[10px] font-bold text-void"
                >
                  <Check size={10} className="inline" /> сохранить
                </button>
              </div>
            </div>
          ) : (
            <div className="break-words">
              <Markdown text={message.content} nodes={nodes} types={types} onOpen={onNodeClick} variant="chat" />
            </div>
          )}
          <Reactions m={message} agents={agents} onReact={onReact} />
          <div className="mt-1 flex items-center justify-end gap-2 text-right text-[10px] font-mono text-zinc-500">
            {onReply && (
              <button onClick={() => onReply(message)} className="hidden text-zinc-600 hover:text-zinc-200 group-hover:inline" title="Ответить на это">
                <CornerUpLeft size={11} />
              </button>
            )}
            {!editing && onEdit && (
              <button onClick={() => { setDraft(message.content); setEditing(true) }} className="hidden text-zinc-600 hover:text-zinc-200 group-hover:inline" title="Править">
                <Pencil size={11} />
              </button>
            )}
            {onDelete && (
              <button onClick={() => { if (confirm('Удалить сообщение из истории?')) onDelete(message.id) }} className="hidden text-zinc-600 hover:text-magenta group-hover:inline" title="Удалить">
                <Trash2 size={11} />
              </button>
            )}
            <span>ты · {time}</span>
          </div>
        </div>
      </div>
    )
  }

  const to =
    message.addressee && message.addressee !== 'room'
      ? message.addressee === 'user'
        ? 'тебе'
        : agents.find((a) => a.id === message.addressee)?.name.split(' ')[0] ?? message.addressee
      : null

  return (
    <div className={`group flex gap-2 ${highlight ? 'rounded-lg ring-1 ring-acid/70' : ''}`}>
      <div className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-sm border text-sm font-bold" style={{ borderColor: color, color }}>
        {AGENT_GLYPH[message.sender_id] ?? '?'}
      </div>
      <div className="min-w-0 max-w-[90%] flex-1">
        <div className="mb-0.5 flex flex-wrap items-baseline gap-x-2">
          <span className="text-xs font-semibold" style={{ color }}>
            {agent?.name.split(' (')[0] ?? message.sender_id}
          </span>
          {to && <span className="font-mono text-[10px] text-zinc-500">→ {to}</span>}
          {message.move && <span className="font-mono text-[9px] uppercase tracking-wider text-zinc-600">{message.move}</span>}
          <span className="text-[10px] font-mono text-zinc-600">{time}</span>
          {canSpeak && !silent && (
            <button
              onClick={() => (speaker.state === 'idle' ? void speaker.play() : speaker.stop())}
              className={`ml-1 rounded p-0.5 ${speaker.state === 'idle' ? 'text-zinc-600 hover:text-zinc-200' : 'text-acid'}`}
              title={speaker.state === 'idle' ? (serverTts ? 'Озвучить' : 'Озвучить голосом браузера') : 'Стоп'}
            >
              {speaker.state === 'idle' ? <Volume2 size={12} /> : speaker.state === 'loading' ? <Volume2 size={12} className="animate-pulse" /> : <VolumeX size={12} />}
            </button>
          )}
          {onDelete && (
            <button onClick={() => { if (confirm('Удалить реплику из истории?')) onDelete(message.id) }} className="hidden rounded p-0.5 text-zinc-700 hover:text-magenta group-hover:inline" title="Удалить из истории">
              <Trash2 size={11} />
            </button>
          )}
        </div>
        {node && <div className="mb-1 flex flex-wrap gap-1">{chip(node, true)}</div>}
        {showEgo && message.ego_thought && (
          <div className="mb-1 rounded border border-line bg-black/50 font-mono text-[11px]">
            <button onClick={() => setOpen((v) => !v)} className="flex w-full items-center gap-1 px-2 py-1 text-left text-zinc-500 hover:text-zinc-300">
              {open ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
              <Terminal size={11} />
              <span className="tracking-widest">мысль</span>
              {!open && <span className="ml-1 truncate normal-case tracking-normal text-zinc-600">{message.ego_thought}</span>}
            </button>
            {open && <div className="whitespace-pre-wrap break-words border-t border-line px-2.5 py-1.5 leading-relaxed text-zinc-400/90">{message.ego_thought}</div>}
          </div>
        )}
        <div
          className={`rounded-lg rounded-tl-sm border px-3 py-2 text-sm leading-relaxed ${silent ? 'border-dashed bg-transparent text-zinc-400' : message.move === 'вставка' ? 'bg-panel/60 text-zinc-200' : 'bg-panel text-zinc-100'}`}
          style={{ borderColor: `${color}55` }}
        >
          <Quote id={message.reply_to} messages={messages} agents={agents} />
          <div className="break-words">
            <Markdown text={message.content} nodes={nodes} types={types} onOpen={onNodeClick} variant="chat" />
          </div>
        </div>
        <div className="flex items-start gap-2">
          <Reactions m={message} agents={agents} onReact={onReact} />
          <div className="ml-auto mt-1 hidden items-center gap-1.5 group-hover:flex">
            {onReply && (
              <button onClick={() => onReply(message)} className="flex items-center gap-0.5 font-mono text-[10px] text-zinc-600 hover:text-zinc-200" title="Ответить на эту реплику">
                <CornerUpLeft size={11} /> ответить
              </button>
            )}
            {onDm && message.channel === '' && (
              <button onClick={() => onDm(message.sender_id)} className="flex items-center gap-0.5 font-mono text-[10px] text-zinc-600 hover:text-zinc-200" title="Написать лично">
                <Lock size={10} /> в личку
              </button>
            )}
          </div>
        </div>
        {mine.length > 0 && (
          <div className="mt-1.5 space-y-1">
            {mine.map((p) => (
              <ProposalCard key={p.id} proposal={p} agents={agents} nodes={nodes} types={types} onAccept={onAcceptProposal ?? (async () => undefined)} onReject={onRejectProposal ?? (async () => undefined)} onOpen={onNodeClick} />
            ))}
          </div>
        )}
        {(message.actions ?? []).length > 0 && (
          <div className="mt-1 flex flex-wrap gap-1">
            {message.actions.map((a, i) => {
              const n = nodes.find((x) => x.id === a.object_id)
              const t = n ? types.find((x) => x.id === n.type_id) : undefined
              return (
                <button
                  key={i}
                  onClick={() => a.object_id && onNodeClick?.(a.object_id)}
                  className="inline-flex max-w-[280px] items-center gap-1 rounded border border-dashed px-1.5 py-0.5 font-mono text-[10px] text-zinc-300 hover:border-acid hover:text-acid"
                  style={{ borderColor: `${color}77` }}
                  title="Действие в библиотеке — открыть объект"
                >
                  <span style={{ color: t?.color ?? color }}>{a.op === 'link' ? '⇄' : a.op === 'update' ? '✎' : a.op === 'append' ? '✍' : a.op === 'note' ? '✎' : a.op === 'search' ? '🔎' : t?.icon ?? '＋'}</span>
                  <span className="truncate">{a.text}</span>
                </button>
              )
            })}
          </div>
        )}
        {mentioned.length > 0 && <div className="mt-1 flex flex-wrap gap-1">{mentioned.map((n) => chip(n))}</div>}
      </div>
    </div>
  )
}
