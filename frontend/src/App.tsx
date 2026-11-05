import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { AlertTriangle, Command, KeyRound, Menu, Minus, PanelRightClose, PanelRightOpen, Plus, RotateCcw, Send, Settings, Users, Zap } from 'lucide-react'
import { useRoom } from './hooks/useRoom'
import { api } from './api'
import { ScenePanel } from './components/ScenePanel'
import { SidePanel, type SideTab } from './components/SidePanel'
import { AgentProfile } from './components/AgentProfile'
import { SettingsModal } from './components/SettingsModal'
import { TypeEditor } from './components/TypeEditor'
import { YardModal } from './components/YardModal'
import { Onboarding } from './components/Onboarding'
import { CommandPalette, type PaletteCommand } from './components/CommandPalette'
import { RoomSwitcher } from './components/RoomSwitcher'
import { DmPanel } from './components/DmPanel'
import { LiveProvider } from './live/LiveContext'

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

function usePersisted<T>(key: string, initial: T): [T, (v: T) => void] {
  const [v, setV] = useState<T>(() => {
    try {
      const raw = localStorage.getItem(key)
      return raw ? (JSON.parse(raw) as T) : initial
    } catch {
      return initial
    }
  })
  const set = (nv: T) => {
    setV(nv)
    try {
      localStorage.setItem(key, JSON.stringify(nv))
    } catch {
      /* ignore */
    }
  }
  return [v, set]
}

export default function App() {
  const room = useRoom()
  const [showEgo, setShowEgo] = usePersisted('triad.ego', true)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [sideOpen, setSideOpen] = usePersisted('triad.side', true)
  const [sideWidth, setSideWidth] = usePersisted('triad.sideWidth', 520)
  const [scale, setScale] = usePersisted('triad.scale', 1)
  const [sideTab, setSideTab] = useState<SideTab>('library')
  const [mobileTab, setMobileTab] = useState<'side' | 'scene'>('scene')
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [contextId, setContextId] = useState<string | null>(null)
  const [profileId, setProfileId] = useState<string | null>(null)
  const [libraryQuery, setLibraryQuery] = useState<{ q: string; nonce: number } | null>(null)
  const [typeEditor, setTypeEditor] = useState<{ open: boolean; id: string | null }>({ open: false, id: null })
  const [yardOpen, setYardOpen] = useState(false)
  const [wizard, setWizard] = useState<boolean | null>(null)
  const [palette, setPalette] = useState(false)
  const [menu, setMenu] = useState(false)
  const [dm, setDm] = useState<string | null>(null)
  const [dmUnread, setDmUnread] = useState<Record<string, number>>({})
  const [toast, setToast] = useState<string | null>(null)
  const isDesktop = useMedia('(min-width: 900px)')
  const dragging = useRef(false)

  // first run
  useEffect(() => {
    if (!room.state || wizard !== null) return
    let seen = false
    try {
      seen = localStorage.getItem('triad.onboarded') === '1'
    } catch {
      /* ignore */
    }
    const st = room.state
    setWizard(!seen && st.settings_editable && st.mock_mode && st.messages.filter((m) => m.sender_id === 'user').length === 0)
  }, [room.state, wizard])

  // UI zoom (Ctrl +/-/0), persisted
  useEffect(() => {
    const root = document.getElementById('root') as HTMLElement | null
    if (root) (root.style as unknown as { zoom: string }).zoom = String(scale)
  }, [scale])

  // DM unread badges
  useEffect(() => {
    if (!room.state) return
    const load = () => void api.dmOverview().then((o) => setDmUnread(Object.fromEntries(o.channels.map((c) => [c.agent, c.unread])))).catch(() => undefined)
    load()
    const t = setInterval(load, 20_000)
    return () => clearInterval(t)
  }, [room.state?.messages.length, room.dms])

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

  const openDm = useCallback(
    (agentId: string) => {
      setDm(agentId)
      void room.loadDm(agentId)
      if (!isDesktop) setMobileTab('scene')
    },
    [room, isDesktop],
  )

  // keyboard shortcuts
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey
      const tag = (e.target as HTMLElement | null)?.tagName
      const typing = tag === 'INPUT' || tag === 'TEXTAREA' || (e.target as HTMLElement | null)?.isContentEditable
      if (mod && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        setPalette((v) => !v)
      } else if (mod && e.key.toLowerCase() === 'b') {
        e.preventDefault()
        setSideOpen(!sideOpen)
      } else if (mod && (e.key === '=' || e.key === '+')) {
        e.preventDefault()
        setScale(Math.min(1.6, Math.round((scale + 0.1) * 10) / 10))
      } else if (mod && e.key === '-') {
        e.preventDefault()
        setScale(Math.max(0.7, Math.round((scale - 0.1) * 10) / 10))
      } else if (mod && e.key === '0') {
        e.preventDefault()
        setScale(1)
      } else if (!typing && e.key === '/' && !mod) {
        e.preventDefault()
        ;(document.querySelector('main textarea') as HTMLTextAreaElement | null)?.focus()
      } else if (e.key === 'Escape' && dm) {
        setDm(null)
      } else if (mod && /^[1-6]$/.test(e.key)) {
        e.preventDefault()
        const tabs: SideTab[] = ['library', 'object', 'graph', 'organism', 'contracts', 'director']
        setSideTab(tabs[Number(e.key) - 1])
        setSideOpen(true)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [sideOpen, scale, dm, setSideOpen, setScale])

  // resizable aside
  useEffect(() => {
    const move = (e: MouseEvent) => {
      if (!dragging.current) return
      const w = Math.min(window.innerWidth * 0.65, Math.max(360, window.innerWidth - e.clientX))
      setSideWidth(Math.round(w))
    }
    const up = () => {
      dragging.current = false
      document.body.style.cursor = ''
      document.querySelector('.resize-handle')?.classList.remove('active')
    }
    window.addEventListener('mousemove', move)
    window.addEventListener('mouseup', up)
    return () => {
      window.removeEventListener('mousemove', move)
      window.removeEventListener('mouseup', up)
    }
  }, [setSideWidth])

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
  const dmAgent = dm ? s.agents.find((a) => a.id === dm) ?? null : null
  const dmTotal = Object.values(dmUnread).reduce((a, b) => a + b, 0)

  const selectNode = (id: string | null) => {
    setSelectedId(id)
    if (id) {
      setContextId(id)
      setSideTab('object')
      setSideOpen(true)
      if (!isDesktop) setMobileTab('side')
    }
  }
  const jumpMessage = (id: number) => {
    setDm(null)
    void room.loadAround(id).then(() => setTimeout(() => document.getElementById(`msg-${id}`)?.scrollIntoView({ block: 'center' }), 80))
  }

  const commands: PaletteCommand[] = [
    { id: 'new-note', title: 'Новая заметка', hint: 'в библиотеку', run: () => void room.createNode('Новая заметка', { type_id: 'note' }) },
    { id: 'new-task', title: 'Новая задача', run: () => void room.createNode('Новая задача', { type_id: 'task' }) },
    { id: 'yard', title: 'Двор: жители и роли', keys: '', run: () => setYardOpen(true) },
    { id: 'settings', title: 'Настройки', hint: 'ключ, модели, окна наружу, бэкап, обновление', run: () => setSettingsOpen(true) },
    { id: 'types', title: 'Типы объектов', hint: 'поля, рейтинги, шаблоны', run: () => setTypeEditor({ open: true, id: null }) },
    { id: 'organism', title: 'Организм', hint: 'пульс библиотеки', keys: 'Ctrl+4', run: () => { setSideTab('organism'); setSideOpen(true) } },
    { id: 'graph', title: 'Граф', keys: 'Ctrl+3', run: () => { setSideTab('graph'); setSideOpen(true) } },
    { id: 'tidy', title: 'Прибрать библиотеку', hint: 'жители разберут самое важное', run: () => void api.tend().then(() => room.refresh()) },
    { id: 'audit', title: 'Аудит библиотеки', hint: 'дубли, пустышки, теги — с подтверждением', run: () => void api.audit().then(() => room.refresh()) },
    { id: 'chronicle', title: 'Написать хронику дня', run: () => void api.chronicle().then(() => room.refresh()) },
    { id: 'snapshot', title: 'Снимок комнаты', hint: 'бэкап в ~/.triad/backups', run: () => void api.createSnapshot().then(() => room.refresh()) },
    { id: 'tick', title: 'Пусть поспорят сами', run: () => void room.tick() },
    { id: 'side', title: sideOpen ? 'Скрыть боковую панель' : 'Показать боковую панель', keys: 'Ctrl+B', run: () => setSideOpen(!sideOpen) },
    { id: 'zoom-in', title: 'Крупнее', keys: 'Ctrl +', run: () => setScale(Math.min(1.6, Math.round((scale + 0.1) * 10) / 10)) },
    { id: 'zoom-out', title: 'Мельче', keys: 'Ctrl −', run: () => setScale(Math.max(0.7, Math.round((scale - 0.1) * 10) / 10)) },
    { id: 'zoom-reset', title: 'Масштаб 100%', keys: 'Ctrl 0', run: () => setScale(1) },
    { id: 'export', title: 'Экспорт библиотеки в .md', run: () => window.open(api.exportUrl(), '_blank') },
    { id: 'wizard', title: 'Мастер первого запуска', run: () => setWizard(true) },
    ...s.agents.filter((a) => a.active !== 0).map((a) => ({ id: `dm-${a.id}`, title: `Написать лично: ${a.name}`, hint: 'личные сообщения', run: () => openDm(a.id) })),
  ]

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
      attachments={s.attachments}
      features={s.features}
      onUpload={room.uploadToNode}
      onUploadFiles={room.uploadFiles}
      onDeleteFile={(id) => void room.deleteFile(id)}
      onAutofill={(id) => room.autofill(id)}
      onGithub={(id) => room.githubInto(id)}
      onEditTypes={(id) => setTypeEditor({ open: true, id: id ?? null })}
      onPollFeeds={room.pollFeeds}
      onTend={(id) => api.tend(id).then((r) => (void room.refresh(), r))}
      onChronicle={() => api.chronicle().then((r) => (void room.refresh(), r))}
      onAudit={() => api.audit().then((r) => (void room.refresh(), r))}
      proposals={s.proposals}
      onAcceptProposal={room.acceptProposal}
      onRejectProposal={room.rejectProposal}
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

  const scene =
    dm && dmAgent ? (
      <DmPanel
        agent={dmAgent}
        agents={s.agents}
        messages={room.dms[dm] ?? []}
        nodes={s.graph.nodes}
        types={s.types}
        busy={room.dmBusy === dm}
        serverTts={s.features.tts}
        onBack={() => setDm(null)}
        onSend={(t, r) => room.sendDm(dm, t, r)}
        onLoadOlder={() => room.loadDm(dm, true)}
        onReact={(id, e) => void room.react(id, e)}
        onDelete={(id) => void room.deleteMessage(id)}
        onNodeClick={selectNode}
      />
    ) : (
      <ScenePanel
        agents={s.agents}
        messages={s.messages}
        nodes={s.graph.nodes}
        types={s.types}
        busy={room.busy}
        speaking={speaking}
        showEgo={showEgo}
        onToggleEgo={() => setShowEgo(!showEgo)}
        disabled={disabled}
        onSend={(t, nodeId, replyTo) => void room.send(t, nodeId, replyTo)}
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
        features={s.features}
        world={s.world}
        onYard={() => setYardOpen(true)}
        messageCount={s.message_count}
        onLoadOlder={room.loadOlder}
        onLoadAround={room.loadAround}
        onEditMessage={(id, c) => void room.editMessage(id, c)}
        onDeleteMessage={(id) => void room.deleteMessage(id)}
        onReact={(id, e) => void room.react(id, e)}
        onDm={openDm}
        proposals={s.proposals}
        onAcceptProposal={room.acceptProposal}
        onRejectProposal={room.rejectProposal}
        onAttach={async (files) => {
          const r = await room.uploadFiles(files)
          const fresh = await api.state()
          return r.ids.map((id) => fresh.graph.nodes.find((n) => n.id === id)?.label ?? id)
        }}
      />
    )

  const iconBtn = 'rounded border border-line p-1 text-zinc-400 hover:border-acid hover:text-acid'

  const live = {
    nodes: s.graph.nodes,
    edges: s.graph.edges,
    types: s.types,
    attachments: s.attachments,
    agents: s.agents,
    actions: {
      openNode: (id: string) => selectNode(id),
      patchNode: (id: string, patch: { body?: string; props?: Record<string, unknown>; label?: string; archived?: boolean }) => void room.patchNode(id, patch),
      createNode: (label: string, opts: { type_id?: string; body?: string; props?: Record<string, unknown> }) => room.createNode(label, opts),
      say: (text: string, nodeId?: string | null) => void room.send(text, nodeId ?? null),
      askAgent: (agentId: string, text: string, nodeId?: string | null) => {
        const a = s.agents.find((x) => x.id === agentId)
        void room.send(`${a ? a.name.split(' ')[0] : agentId}, ${text}`, nodeId ?? null)
      },
      dm: (agentId: string) => openDm(agentId),
      tend: (nodeId?: string) => api.tend(nodeId).then((r) => (void room.refresh(), r)),
      audit: () => api.audit().then((r) => (void room.refresh(), r)),
      chronicle: () => api.chronicle().then((r) => (void room.refresh(), r)),
      snapshot: () => api.createSnapshot(),
      tick: () => void room.tick(),
      notify: (text: string) => {
        setToast(text)
        window.setTimeout(() => setToast(null), 3500)
      },
    },
  }

  return (
    <LiveProvider value={live}>
    <div className="scanlines relative flex flex-col bg-void" style={{ height: 'var(--app-height, 100%)' }}>
      {toast && <div className="fixed bottom-16 left-1/2 z-[80] -translate-x-1/2 rounded border border-acid/60 bg-void px-3 py-1.5 font-mono text-xs text-acid shadow-glow">{toast}</div>}
      <header className="relative z-20 flex items-center gap-2 border-b border-line bg-void/95 px-3 py-1.5" style={{ paddingTop: 'calc(0.375rem + var(--tg-safe-top))' }}>
        <div className="relative">
          <button onClick={() => setMenu((v) => !v)} className={iconBtn} title="Меню">
            <Menu size={13} />
          </button>
          {menu && (
            <div className="absolute left-0 top-full z-50 mt-1 w-64 rounded border border-line bg-void p-1 text-xs shadow-2xl" onMouseLeave={() => setMenu(false)}>
              {[
                ['Палитра команд', 'Ctrl+K', () => setPalette(true)],
                ['Двор: жители', '', () => setYardOpen(true)],
                ['Типы объектов', '', () => setTypeEditor({ open: true, id: null })],
                ['Организм', 'Ctrl+4', () => { setSideTab('organism'); setSideOpen(true) }],
                ['Настройки', '', () => setSettingsOpen(true)],
                ['Снимок комнаты', '', () => void api.createSnapshot().then(() => room.refresh())],
                ['Экспорт библиотеки .md', '', () => window.open(api.exportUrl(), '_blank')],
                ['Мастер первого запуска', '', () => setWizard(true)],
                ['Стереть комнату…', '', () => { if (confirm('Стереть комнату и заселить Триаду заново?')) void room.reset() }],
              ].map(([t, k, fn]) => (
                <button
                  key={t as string}
                  onClick={() => {
                    setMenu(false)
                    ;(fn as () => void)()
                  }}
                  className="flex w-full items-center justify-between rounded px-2 py-1 text-left text-zinc-300 hover:bg-zinc-800 hover:text-zinc-100"
                >
                  <span>{t as string}</span>
                  {k && <kbd className="font-mono text-[9px] text-zinc-600">{k as string}</kbd>}
                </button>
              ))}
              <div className="mt-1 border-t border-line px-2 pt-1 font-mono text-[9px] text-zinc-600">/ — в композер · Ctrl+B панель · Ctrl +/− масштаб · Ctrl+1…6 вкладки</div>
            </div>
          )}
        </div>
        <Zap size={15} className="text-acid" />
        <span className="hidden font-mono text-xs font-bold tracking-[0.25em] text-zinc-100 sm:inline">TRIAD CANVAS</span>
        <RoomSwitcher />
        <span className="hidden font-mono text-[10px] text-zinc-600 lg:inline">
          {liveCount} объектов · {s.types.length} типов
          {s.usage.daily_budget > 0 && ` · вызовов ${s.usage.calls_today}/${s.usage.daily_budget}`}
        </span>
        <div className="ml-auto flex items-center gap-1.5 font-mono text-[10px]">
          {s.source_update && s.source_update.behind > 0 && (
            <button onClick={() => setSettingsOpen(true)} className="rounded border border-acid/60 px-1.5 py-0.5 text-acid hover:bg-acid/10" title={`Новые коммиты в origin/${s.source_update.branch}: обновиться из приложения`}>
              ↑ +{s.source_update.behind}
            </button>
          )}
          {s.mock_mode && (
            <button onClick={() => setSettingsOpen(true)} className="rounded border border-magenta/50 px-1.5 py-0.5 text-magenta hover:bg-magenta/10" title="Нет ключа OpenRouter — ответы шаблонные">
              MOCK
            </button>
          )}
          <span className={room.live ? 'text-acid' : 'text-zinc-600'} title={room.live ? 'живое соединение' : 'нет соединения'}>
            {room.live ? '●' : '○'}
          </span>
          <button onClick={() => setPalette(true)} className={`${iconBtn} hidden sm:flex items-center gap-1`} title="Палитра команд (Ctrl+K)">
            <Command size={11} /> <span>K</span>
          </button>
          <div className="hidden items-center gap-0.5 sm:flex" title="Масштаб интерфейса">
            <button onClick={() => setScale(Math.max(0.7, Math.round((scale - 0.1) * 10) / 10))} className={iconBtn}>
              <Minus size={11} />
            </button>
            <button onClick={() => setScale(1)} className="w-9 text-center text-zinc-500 hover:text-zinc-200">
              {Math.round(scale * 100)}%
            </button>
            <button onClick={() => setScale(Math.min(1.6, Math.round((scale + 0.1) * 10) / 10))} className={iconBtn}>
              <Plus size={11} />
            </button>
          </div>
          <button onClick={() => setYardOpen(true)} className={`${iconBtn} relative`} title="Двор: жители, роли, модели, голоса">
            <Users size={12} />
            {dmTotal > 0 && <span className="absolute -right-1 -top-1 rounded-full bg-acid px-1 text-[8px] font-bold text-void">{dmTotal}</span>}
          </button>
          <button onClick={() => setSettingsOpen(true)} className={iconBtn} title="Настройки">
            <Settings size={12} />
          </button>
          <button
            onClick={() => {
              if (confirm('Стереть комнату и заселить Триаду заново?')) void room.reset()
            }}
            className="hidden rounded border border-line p-1 text-zinc-500 hover:border-magenta hover:text-magenta sm:block"
            title="Новая комната"
          >
            <RotateCcw size={12} />
          </button>
          {isDesktop && (
            <button onClick={() => setSideOpen(!sideOpen)} className="rounded border border-line p-1 text-zinc-400 hover:text-zinc-100" title={sideOpen ? 'Свернуть панель (Ctrl+B)' : 'Показать панель (Ctrl+B)'}>
              {sideOpen ? <PanelRightClose size={12} /> : <PanelRightOpen size={12} />}
            </button>
          )}
        </div>
      </header>

      {s.restore_pending && (
        <div className="flex items-center gap-2 border-b border-acid/40 bg-acid/10 px-3 py-1 text-xs text-acid">
          Подготовлено восстановление из бэкапа — перезапусти приложение, чтобы применить.
          <button onClick={() => void api.restoreCancel().then(() => room.refresh())} className="ml-auto text-[10px] underline">
            отменить
          </button>
        </div>
      )}
      {s.update_status.last && !s.update_status.running && (
        <div className={`flex items-center gap-2 border-b px-3 py-1 text-xs ${s.update_status.last.startsWith('ok') ? 'border-acid/40 bg-acid/10 text-acid' : 'border-magenta/40 bg-magenta/10 text-magenta'}`}>
          {s.update_status.last.startsWith('ok') ? `Обновлено: ${s.update_status.last.split(' ')[1] ?? ''} · v${s.version}` : `Обновление не удалось: ${s.update_status.last.replace(/^fail\s*/, '')} (лог в ~/.triad/update.log)`}
          <button
            onClick={() => {
              void api.updateAck().then(() => room.refresh())
            }}
            className="ml-auto text-[10px] underline"
          >
            ок
          </button>
        </div>
      )}
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
          {sideOpen && (
            <>
              <div
                className="resize-handle shrink-0"
                onMouseDown={(e) => {
                  e.preventDefault()
                  dragging.current = true
                  document.body.style.cursor = 'col-resize'
                  ;(e.currentTarget as HTMLElement).classList.add('active')
                }}
                onDoubleClick={() => setSideWidth(520)}
                title="Тянуть, чтобы менять ширину · двойной клик — сброс"
              />
              <aside className="shrink-0 border-l border-line" style={{ width: sideWidth }}>
                {side}
              </aside>
            </>
          )}
        </div>
      ) : (
        <>
          <div className="min-h-0 flex-1">{mobileTab === 'side' ? side : scene}</div>
          <nav className="grid grid-cols-2 border-t border-line bg-void font-mono text-[11px]" style={{ paddingBottom: 'var(--tg-safe-bottom)' }}>
            <button onClick={() => setMobileTab('scene')} className={`py-2 ${mobileTab === 'scene' ? 'text-acid' : 'text-zinc-500'}`}>
              ☰ {dm ? 'Личка' : 'Чат'}
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
          onDm={() => {
            setProfileId(null)
            openDm(profile.id)
          }}
          dmUnread={dmUnread[profile.id] ?? 0}
        />
      )}
      <SettingsModal
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        onSaved={() => void room.refresh()}
        sourceUpdate={s.source_update}
        restorePending={s.restore_pending}
        repoDir={s.repo_dir}
        exePath={s.exe_path}
        onWizard={() => {
          setSettingsOpen(false)
          setWizard(true)
        }}
      />
      <Onboarding
        open={wizard === true}
        agents={s.agents}
        hasKey={!s.mock_mode}
        model={s.model}
        onClose={() => setWizard(false)}
        onDone={() => {
          setWizard(false)
          void room.refresh()
        }}
        onCreateAgent={room.createAgent}
        onPatchAgent={room.patchAgent}
      />
      <YardModal open={yardOpen} agents={s.agents} roomModel={s.model} onClose={() => setYardOpen(false)} onCreate={room.createAgent} onPatch={room.patchAgent} onDm={(id) => { setYardOpen(false); openDm(id) }} dmUnread={dmUnread} />
      <TypeEditor
        open={typeEditor.open}
        initialId={typeEditor.id}
        types={s.types}
        counts={Object.fromEntries(s.types.map((t) => [t.id, s.graph.nodes.filter((n) => n.type_id === t.id && n.archived === 0).length]))}
        onClose={() => setTypeEditor({ open: false, id: null })}
        onSave={(id, patch) => void room.updateType(id, patch)}
        onCreate={(name) => void room.createType(name)}
        onDelete={(id) => void room.deleteType(id)}
      />
      <CommandPalette open={palette} onClose={() => setPalette(false)} nodes={s.graph.nodes} types={s.types} agents={s.agents} commands={commands} onOpenNode={selectNode} onJumpMessage={jumpMessage} />
    </div>
    </LiveProvider>
  )
}
