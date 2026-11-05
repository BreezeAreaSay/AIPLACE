import { useEffect, useMemo, useState } from 'react'
import { AlertTriangle, KeyRound, PanelRightClose, PanelRightOpen, RotateCcw, Send, Settings, Zap } from 'lucide-react'
import { useRoom } from './hooks/useRoom'
import { Canvas } from './components/Canvas'
import { NodePanel } from './components/NodePanel'
import { ScenePanel } from './components/ScenePanel'
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
  const [sceneOpen, setSceneOpen] = useState(true)
  const [mobileTab, setMobileTab] = useState<'canvas' | 'scene'>('scene')
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [contextId, setContextId] = useState<string | null>(null)
  const [profileId, setProfileId] = useState<string | null>(null)
  const isDesktop = useMedia('(min-width: 900px)')

  const speaking = useMemo(() => {
    const d = room.busy?.detail ?? ''
    if (room.busy?.stage !== 'agent') return null
    return room.state?.agents.find((a) => d.startsWith(a.name))?.id ?? null
  }, [room.busy, room.state])

  // Nodes touched in the last round glow for a moment.
  const hotIds = useMemo(() => {
    const s = room.state
    if (!s) return new Set<string>()
    const ids = new Set<string>()
    const d = s.last_director?.output
    for (const n of d?.graph_mutations.nodes ?? []) ids.add(n.id)
    for (const u of d?.node_updates ?? []) ids.add(u.id)
    for (const m of s.messages.slice(-6)) if (m.node_id) ids.add(m.node_id)
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

  const selectNode = (id: string | null) => {
    setSelectedId(id)
    if (id) setContextId(id)
    if (id && !isDesktop) setMobileTab('canvas')
  }

  const canvas = (
    <div className="relative h-full w-full">
      <Canvas
        graph={s.graph}
        notes={s.node_notes}
        agents={s.agents}
        selectedId={selectedId}
        hotIds={hotIds}
        onSelect={selectNode}
        onCreate={(label, pos) => void room.createNode(label, pos)}
        onMove={(id, x, y) => void room.patchNode(id, { x, y })}
        onConnect={(a, b) => void room.createEdge(a, b)}
        toolbarOffset={selected && isDesktop ? 380 : 0}
      />
      {selected && (
        <div className="absolute inset-y-0 left-0 z-10 w-full md:w-auto">
          <NodePanel
            node={selected}
            graph={s.graph}
            notes={s.node_notes}
            agents={s.agents}
            disabled={disabled}
            onClose={() => setSelectedId(null)}
            onPatch={(id, patch) => void room.patchNode(id, patch)}
            onDelete={(id) => {
              void room.deleteNode(id)
              if (contextId === id) setContextId(null)
            }}
            onAsk={(id, mode, msg) => {
              void room.askNode(id, mode, msg)
              if (!isDesktop) setMobileTab('scene')
            }}
            onAddNote={(id, text) => void room.addNote(id, text)}
            onDeleteEdge={(id) => void room.deleteEdge(id)}
            onSelect={selectNode}
          />
        </div>
      )}
    </div>
  )

  const scene = (
    <ScenePanel
      agents={s.agents}
      messages={s.messages}
      nodes={s.graph.nodes}
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
      contracts={s.contracts}
      now={s.now}
      onResolve={(id, st) => void room.resolveContract(id, st)}
      director={s.last_director}
      directorModel={s.director_model}
      relations={s.relations}
      memories={s.memories}
      events={s.events}
      mood={mood}
    />
  )

  return (
    <div className="scanlines relative flex flex-col bg-void" style={{ height: 'var(--app-height, 100%)' }}>
      <header className="relative z-20 flex items-center gap-2 border-b border-line bg-void/95 px-3 py-1.5" style={{ paddingTop: 'calc(0.375rem + var(--tg-safe-top))' }}>
        <Zap size={15} className="text-acid" />
        <span className="font-mono text-xs font-bold tracking-[0.25em] text-zinc-100">TRIAD CANVAS</span>
        <span className="hidden font-mono text-[10px] text-zinc-600 sm:inline">
          {s.graph.nodes.length} узлов · {s.node_notes.length} пометок{s.room_id !== 'local' ? ` · ${s.room_id}` : ''}
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
            <button onClick={() => setSceneOpen((v) => !v)} className="rounded border border-line p-1 text-zinc-400 hover:text-zinc-100" title={sceneOpen ? 'Свернуть сцену' : 'Показать сцену'}>
              {sceneOpen ? <PanelRightClose size={12} /> : <PanelRightOpen size={12} />}
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
          <main className="min-w-0 flex-1">{canvas}</main>
          {sceneOpen && <aside className="w-[460px] shrink-0 border-l border-line">{scene}</aside>}
        </div>
      ) : (
        <>
          <div className="min-h-0 flex-1">{mobileTab === 'canvas' ? canvas : scene}</div>
          <nav className="grid grid-cols-2 border-t border-line bg-void font-mono text-[11px]" style={{ paddingBottom: 'var(--tg-safe-bottom)' }}>
            <button onClick={() => setMobileTab('canvas')} className={`py-2 ${mobileTab === 'canvas' ? 'text-acid' : 'text-zinc-500'}`}>
              ◇ Холст · {s.graph.nodes.length}
            </button>
            <button onClick={() => setMobileTab('scene')} className={`py-2 ${mobileTab === 'scene' ? 'text-acid' : 'text-zinc-500'}`}>
              ☰ Сцена
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
