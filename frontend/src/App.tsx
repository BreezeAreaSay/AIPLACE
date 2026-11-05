import { useEffect, useMemo, useState } from 'react'
import { AlertTriangle, KeyRound, PanelRightClose, PanelRightOpen, RotateCcw, Send, Settings, Zap } from 'lucide-react'
import { useRoom } from './hooks/useRoom'
import { ScenePanel } from './components/ScenePanel'
import { SidePanel, type SideTab } from './components/SidePanel'
import { AgentProfile } from './components/AgentProfile'
import { SettingsModal } from './components/SettingsModal'

function useMedia(query: string) {
  const [m, setM] = useState(() => window.matchMedia(query).matches)
  useEffect(() => {
    const mq = window.matchMedia(query)
    const fn = () => setM(mq.matches)
    mq.addEventListener('change', fn)
    return () => mq.removeEventListener('change', fn)
  }, [query])
  return m
}

export default function App() {
  const room = useRoom()
  const [showEgo, setShowEgo] = useState(true)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [sideOpen, setSideOpen] = useState(true)
  const [sideTab, setSideTab] = useState<SideTab>('library')
  const [mobileTab, setMobileTab] = useState<'side' | 'scene'>('scene')
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [contextId, setContextId] = useState<string | null>(null)
  const [profileId, setProfileId] = useState<string | null>(null)
  const [libraryQuery, setLibraryQuery] = useState<{ q: string; nonce: number } | null>(null)
  const isDesktop = useMedia('(min-width: 900px)')

  const unread = useMemo(() => {
    const st = room.state
    if (!st) return 0
    return st.messages.filter((m) => m.id > st.last_seen_message_id && m.sender_id !== 'user' && m.sender_id !== 'system').length
  }, [room.state])
  useEffect(() => {
    document.title = unread > 0 ? `(${unread}) Triad Canvas` : 'Triad Canvas'
  }, [unread])

  const speaking = useMemo(() => {
    const d = room.busy?.detail ?? ''
    if (room.busy?.stage !== 'agent') return null
    return room.state?.agents.find((a) => d.startsWith(a.name))?.id ?? null
  }, [room.busy, room.state])

  // Objects touched in the last round glow for a moment on the graph.
  const hotIds = useMemo(() => {
    const s = room.state
    if (!s) return new Set<string>()
    const ids = new Set<string>()
    const d = s.last_director?.output
    for (const n of d?.graph_mutations.nodes ?? []) ids.add(n.id)
    for (const u of d?.node_updates ?? []) ids.add(u.id)
    for (const m of s.messages.slice(-6)) {
      if (m.node_id) ids.add(m.node_id)
      for (const o of m.objects ?? []) ids.add(o)
    }
    return ids
  }, [room.state])

  if (room.gate) {
    const link = room.gate.bot ? `https://t.me/${room.gate.bot}` : null
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4 px-6 text-center font-mono text-sm text-zinc-400">
        <div className="text-2xl tracking-[0.3em] text-acid">TRIAD CANVAS</div>
        <div className="max-w-sm text-zinc-300">{room.gate.message}</div>
        {link ? (
          <a href={link} className="flex items-center gap-2 rounded bg-acid px-4 py-2 font-bold text-void hover:shadow-glow">
            <Send size={16} /> Открыть в Telegram
          </a>
        ) : (
          <div className="text-xs text-zinc-600">Бот ещё не представился серверу — открой мини-апп из Telegram.</div>
        )}
      </div>
    )
  }

  if (!room.state) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 font-mono text-sm text-zinc-500">
        <span className="animate-pulseLine text-acid">▮▮▮</span>
        {room.error ? (
          <div className="max-w-sm text-center">
            <div className="text-magenta">Бэкенд не отвечает: {room.error}</div>
            <button onClick={() => void room.refresh()} className="mt-3 rounded border border-line px-3 py-1 text-xs hover:border-acid hover:text-acid">
              повторить
            </button>
          </div>
        ) : (
          'подключаюсь к комнате…'
        )}
      </div>
    )
  }

  const s = room.state
  const disabled = room.busy !== null
  const selected = selectedId ? s.graph.nodes.find((n) => n.id === selectedId) ?? null : null
  const contextNode = contextId ? s.graph.nodes.find((n) => n.id === contextId) ?? null : null
  const profile = profileId ? s.agents.find((a) => a.id === profileId) ?? null : null
  const mood = s.last_director?.output.room_mood ?? null
  const liveCount = s.graph.nodes.filter((n) => n.archived === 0).length

  const selectNode = (id: string | null) => {
    setSelectedId(id)
    if (id) {
      setContextId(id)
      setSideTab('object')
      setSideOpen(true)
      if (!isDesktop) setMobileTab('side')
    }
  }

  const side = (
    <SidePanel
      tab={sideTab}
      onTab={setSideTab}
      graph={s.graph}
      types={s.types}
      notes={s.node_notes}
      agents={s.agents}
      selected={selected}
      hotIds={hotIds}
      disabled={disabled}
      onSelect={selectNode}
      onCreate={(label, opts) => void room.createNode(label, opts)}
      onImport={room.importMd}
      libraryQuery={libraryQuery}
      onCreateType={(name) => void room.createType(name)}
      onPatch={(id, patch) => void room.patchNode(id, patch)}
      onDelete={(id) => {
        void room.deleteNode(id)
        if (contextId === id) setContextId(null)
        setSelectedId(null)
        setSideTab('library')
      }}
      onAsk={(id, mode, msg) => {
        void room.askNode(id, mode, msg)
        if (!isDesktop) setMobileTab('scene')
      }}
      onAddNote={(id, text) => void room.addNote(id, text)}
      onDeleteEdge={(id) => void room.deleteEdge(id)}
      onConnect={(a, b) => void room.createEdge(a, b)}
      onMove={(id, x, y) => void room.patchNode(id, { x, y })}
      contracts={s.contracts}
      now={s.now}
      onResolve={(id, st) => void room.resolveContract(id, st)}
      onPromise={(t) => void room.send(t)}
      director={s.last_director}
      directorModel={s.director_model}
      relations={s.relations}
      memories={s.memories}
      events={s.events}
    />
  )

  const scene = (
    <ScenePanel
      agents={s.agents}
      messages={s.messages}
      nodes={s.graph.nodes}
      types={s.types}
      busy={room.busy}
      speaking={speaking}
      showEgo={showEgo}
      onToggleEgo={() => setShowEgo((v) => !v)}
      disabled={disabled}
      onSend={(t, nodeId) => void room.send(t, nodeId)}
      onTick={() => void room.tick()}
      contextNode={contextNode}
      onClearContext={() => setContextId(null)}
      onNodeClick={selectNode}
      onOpenAgent={setProfileId}
      mood={mood}
      lastSeen={s.last_seen_message_id}
      streaming={room.streaming}
      onSeen={room.markSeen}
      onCapture={(label, typeId, body) => {
        void room.createNode(label, { type_id: typeId, body: body || undefined })
      }}
      onSearch={(q) => {
        setLibraryQuery({ q, nonce: Date.now() })
        setSideTab('library')
        setSideOpen(true)
        if (!isDesktop) setMobileTab('side')
      }}
    />
  )


  return (
    <div className="scanlines relative flex flex-col bg-void" style={{ height: 'var(--app-height, 100%)' }}>
      <header className="relative z-20 flex items-center gap-2 border-b border-line bg-void/95 px-3 py-1.5" style={{ paddingTop: 'calc(0.375rem + var(--tg-safe-top))' }}>
        <Zap size={15} className="text-acid" />
        <span className="font-mono text-xs font-bold tracking-[0.25em] text-zinc-100">TRIAD CANVAS</span>
        <span className="hidden font-mono text-[10px] text-zinc-600 sm:inline">
          {liveCount} объектов · {s.types.length} типов{s.room_id !== 'local' ? ` · ${s.room_id}` : ''}
          {s.usage.daily_budget > 0 && ` · вызовов сегодня ${s.usage.calls_today}/${s.usage.daily_budget}`}
          {s.heartbeat_minutes > 0 && ` · пульс каждые ${s.heartbeat_minutes} мин тишины`}
        </span>
        <div className="ml-auto flex items-center gap-1.5 font-mono text-[10px]">
          {s.update && (
            <button onClick={() => setSettingsOpen(true)} className="hidden rounded border border-acid/60 px-1.5 py-0.5 text-acid sm:inline" title="Доступна новая версия">
              ↑ v{s.update.latest}
            </button>
          )}
          {s.mock_mode && (
            <button onClick={() => setSettingsOpen(true)} className="rounded border border-magenta/50 px-1.5 py-0.5 text-magenta hover:bg-magenta/10" title="Нет ключа OpenRouter — ответы шаблонные">
              MOCK LLM
            </button>
          )}
          <span className={room.live ? 'text-acid' : 'text-zinc-600'}>{room.live ? '● live' : '○ offline'}</span>
          <button onClick={() => setSettingsOpen(true)} className="rounded border border-line p-1 text-zinc-400 hover:border-acid hover:text-acid" title="Ключ и модели">
            <Settings size={12} />
          </button>
          <button
            onClick={() => {
              if (confirm('Стереть комнату и заселить Триаду заново?')) void room.reset()
            }}
            className="rounded border border-line p-1 text-zinc-500 hover:border-magenta hover:text-magenta"
            title="Новая комната"
          >
            <RotateCcw size={12} />
          </button>
          {isDesktop && (
            <button onClick={() => setSideOpen((v) => !v)} className="rounded border border-line p-1 text-zinc-400 hover:text-zinc-100" title={sideOpen ? 'Свернуть библиотеку' : 'Показать библиотеку'}>
              {sideOpen ? <PanelRightClose size={12} /> : <PanelRightOpen size={12} />}
            </button>
          )}
        </div>
      </header>

      {room.error && (
        <div className="flex items-center gap-2 border-b border-magenta/40 bg-magenta/10 px-3 py-1 text-xs text-magenta">
          <AlertTriangle size={14} /> {room.error}
        </div>
      )}
      {s.mock_mode && (
        <button onClick={() => setSettingsOpen(true)} className="flex items-center gap-2 border-b border-acid/30 bg-acid/5 px-3 py-1 text-left text-[11px] text-zinc-300 hover:bg-acid/10">
          <KeyRound size={13} className="shrink-0 text-acid" />
          <span>
            Комната в <b className="text-magenta">mock-режиме</b>: реплики шаблонные.{' '}
            {s.settings_editable ? <span className="text-acid underline">Вставить ключ OpenRouter →</span> : <span>на сервере: <code>triad key sk-or-…</code></span>}
          </span>
        </button>
      )}

      {isDesktop ? (
        <div className="flex min-h-0 flex-1">
          <main className="flex min-w-0 flex-1 justify-center">
            <div className="flex h-full w-full max-w-3xl flex-col">{scene}</div>
          </main>
          {sideOpen && <aside className="w-[520px] shrink-0 border-l border-line">{side}</aside>}
        </div>
      ) : (
        <>
          <div className="min-h-0 flex-1">{mobileTab === 'side' ? side : scene}</div>
          <nav className="grid grid-cols-2 border-t border-line bg-void font-mono text-[11px]" style={{ paddingBottom: 'var(--tg-safe-bottom)' }}>
            <button onClick={() => setMobileTab('scene')} className={`py-2 ${mobileTab === 'scene' ? 'text-acid' : 'text-zinc-500'}`}>
              ☰ Чат
            </button>
            <button onClick={() => setMobileTab('side')} className={`py-2 ${mobileTab === 'side' ? 'text-acid' : 'text-zinc-500'}`}>
              ◇ Библиотека · {liveCount}
            </button>
          </nav>
        </>
      )}

      {profile && (
        <AgentProfile
          agent={profile}
          agents={s.agents}
          memory={s.memories.find((m) => m.agent_id === profile.id)}
          relations={s.relations}
          messages={s.messages}
          onClose={() => setProfileId(null)}
        />
      )}
      <SettingsModal open={settingsOpen} onClose={() => setSettingsOpen(false)} onSaved={() => void room.refresh()} />
    </div>
  )
}
