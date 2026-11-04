import { useEffect, useMemo, useState } from 'react'
import { AlertTriangle, PanelRightOpen } from 'lucide-react'
import { useRoom } from './hooks/useRoom'
import { TopBar } from './components/TopBar'
import { ChatStream } from './components/ChatStream'
import { Composer } from './components/Composer'
import { GraphPanel } from './components/GraphPanel'
import { Drawer } from './components/Drawer'

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
  const [drawer, setDrawer] = useState(false)
  const isDesktop = useMedia('(min-width: 768px)')

  const speaking = useMemo(() => {
    const d = room.busy?.detail ?? ''
    if (room.busy?.stage !== 'agent') return null
    return room.state?.agents.find((a) => d.startsWith(a.name))?.id ?? null
  }, [room.busy, room.state])

  if (!room.state) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 font-mono text-sm text-zinc-500">
        <span className="animate-pulseLine text-acid">▮▮▮</span>
        {room.error ? (
          <div className="max-w-sm text-center">
            <div className="text-magenta">Бэкенд не отвечает: {room.error}</div>
            <div className="mt-2 text-xs text-zinc-600">
              Запусти <code className="text-zinc-300">cargo run</code> в <code className="text-zinc-300">backend/</code> (порт 3000).
            </div>
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

  const panel = (
    <GraphPanel
      graph={s.graph}
      graphMermaid={s.graph_mermaid}
      schema={s.schema}
      director={s.last_director}
      agents={s.agents}
      directorModel={s.director_model}
      disabled={disabled}
      onTick={room.tick}
      onSchema={room.synthesize}
    />
  )

  return (
    <div className="scanlines relative flex h-full flex-col bg-void">
      <TopBar
        agents={s.agents}
        speaking={speaking}
        live={room.live}
        mockMode={s.mock_mode}
        model={s.model}
        onReset={room.reset}
        compact={!isDesktop}
      />

      <div className="flex min-h-0 flex-1">
        {/* Chat column — the Telegram Mini App "glass" */}
        <main className="flex min-h-0 w-full flex-col md:w-[440px] md:shrink-0 md:border-r md:border-line lg:w-[500px]">
          {room.error && (
            <div className="flex items-center gap-2 border-b border-magenta/40 bg-magenta/10 px-3 py-1.5 text-xs text-magenta">
              <AlertTriangle size={14} /> {room.error}
            </div>
          )}
          <ChatStream messages={s.messages} agents={s.agents} showEgo={showEgo} busy={room.busy} />
          <Composer disabled={disabled} onSend={room.send} onTick={room.tick} onSchema={room.synthesize} showEgo={showEgo} onToggleEgo={() => setShowEgo((v) => !v)} />
        </main>

        {/* Desktop inspector */}
        <aside className="hidden min-h-0 flex-1 md:block">{panel}</aside>
      </div>

      {/* Mobile: floating button + bottom sheet */}
      <button
        onClick={() => setDrawer(true)}
        className="fixed right-3 z-30 flex items-center gap-1.5 rounded-full border border-acid/60 bg-void/90 px-3 py-2 font-mono text-[11px] text-acid shadow-glow md:hidden"
        style={{ bottom: 'calc(7.5rem + var(--tg-safe-bottom))' }}
      >
        <PanelRightOpen size={14} /> граф · {s.graph.nodes.length}
      </button>
      <Drawer open={drawer && !isDesktop} onClose={() => setDrawer(false)} title="Доска комнаты">
        {panel}
      </Drawer>
    </div>
  )
}
