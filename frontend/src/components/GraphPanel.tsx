import { useState } from 'react'
import { Maximize2, Scan } from 'lucide-react'
import type { Agent, DirectorLogEntry, Graph, SchemaDoc } from '../types'
import { AGENT_COLORS } from '../types'
import { MermaidView } from './MermaidView'
import { DirectorInspector } from './DirectorInspector'

type Tab = 'graph' | 'schema' | 'director'

interface Props {
  graph: Graph
  graphMermaid: string
  schema: SchemaDoc | null
  director: DirectorLogEntry | null
  agents: Agent[]
  directorModel: string
  disabled: boolean
  onTick: () => void
  onSchema: () => void
  initialTab?: Tab
}

export function GraphPanel({ graph, graphMermaid, schema, director, agents, directorModel, disabled, onTick, onSchema, initialTab = 'graph' }: Props) {
  const [tab, setTab] = useState<Tab>(initialTab)
  const [mode, setMode] = useState<'fit' | 'natural'>(() => (window.innerWidth < 768 ? 'natural' : 'fit'))
  const tabs: { id: Tab; label: string }[] = [
    { id: 'graph', label: `Живой граф · ${graph.nodes.length}/${graph.edges.length}` },
    { id: 'schema', label: 'Схема' },
    { id: 'director', label: 'Режиссёр' },
  ]
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex items-center gap-1 border-b border-line px-2 pt-2">
        {tabs.map((t) => (
          <button
            key={t.id}
            onClick={() => setTab(t.id)}
            className={`rounded-t border-b-2 px-3 py-1.5 font-mono text-[11px] transition ${
              tab === t.id ? 'border-acid text-acid' : 'border-transparent text-zinc-500 hover:text-zinc-300'
            }`}
          >
            {t.label}
          </button>
        ))}
        <div className="ml-auto flex items-center gap-1.5 pb-1">
          <button
            onClick={() => setMode((m) => (m === 'fit' ? 'natural' : 'fit'))}
            className="flex items-center gap-1 rounded border border-line px-2 py-1 font-mono text-[11px] text-zinc-400 hover:text-zinc-100"
            title={mode === 'fit' ? 'Показать 1:1 (скролл)' : 'Вписать в панель'}
          >
            {mode === 'fit' ? <Scan size={12} /> : <Maximize2 size={12} />} {mode === 'fit' ? 'fit' : '1:1'}
          </button>
          <button
            onClick={onTick}
            disabled={disabled}
            className="hidden items-center gap-1 rounded border border-acid/50 px-2 py-1 font-mono text-[11px] text-acid hover:bg-acid/10 disabled:opacity-40 lg:flex"
          >
            🎲 Автономный тик (пусть поспорят)
          </button>
          <button
            onClick={onSchema}
            disabled={disabled}
            className="hidden items-center gap-1 rounded border border-cyan/50 px-2 py-1 font-mono text-[11px] text-cyan hover:bg-cyan/10 disabled:opacity-40 lg:flex"
          >
            ⚡ Собрать схему / Спарринг
          </button>
        </div>
      </div>

      <div className="relative min-h-0 flex-1 overflow-auto">
        {tab === 'graph' && (
          <div className="p-3">
            <Legend />
            <MermaidView code={graphMermaid} className="mt-2" mode={mode} />
            {graph.nodes.length === 0 && (
              <p className="mt-3 text-center font-mono text-[11px] text-zinc-600">
                Узлы появляются сами: Режиссёр извлекает их из каждой реплики и спора.
              </p>
            )}
          </div>
        )}
        {tab === 'schema' &&
          (schema ? (
            <div className="space-y-3 p-3">
              <div>
                <h3 className="text-sm font-semibold text-zinc-100">{schema.title}</h3>
                <div className="font-mono text-[10px] text-zinc-600">{new Date(schema.created_at).toLocaleString('ru-RU')}</div>
              </div>
              <Legend />
              <MermaidView code={schema.mermaid} mode={mode} />
              {schema.annotations.length > 0 && (
                <div className="space-y-1.5">
                  {schema.annotations.map((a, i) => (
                    <div key={i} className="rounded border-l-2 bg-panel px-2.5 py-1.5 text-xs" style={{ borderColor: AGENT_COLORS[a.author] ?? '#666' }}>
                      <span className="mr-1.5 font-mono text-[10px]" style={{ color: AGENT_COLORS[a.author] }}>
                        {agents.find((x) => x.id === a.author)?.name.split(' (')[0] ?? a.author}:
                      </span>
                      <span className="text-zinc-300">{a.text}</span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          ) : (
            <div className="p-4 text-center font-mono text-xs text-zinc-500">
              Схемы ещё нет. Нажми «⚡ Собрать схему» — Триада превратит спор в структурированную блок-схему с язвительными аннотациями.
            </div>
          ))}
        {tab === 'director' && <DirectorInspector entry={director} agents={agents} directorModel={directorModel} />}
      </div>
    </div>
  )
}

function Legend() {
  const items = [
    ['physicist', 'Физик'],
    ['lit_teacher', 'Литератор'],
    ['graffiti', 'Граффитист'],
    ['user', 'Игрок'],
  ] as const
  return (
    <div className="flex flex-wrap gap-3 font-mono text-[10px] text-zinc-500">
      {items.map(([id, label]) => (
        <span key={id} className="flex items-center gap-1">
          <span className="inline-block h-2 w-2 rounded-sm border-2" style={{ borderColor: AGENT_COLORS[id] }} />
          {label}
        </span>
      ))}
      <span className="ml-auto">▭ концепт · ▭▭ задача · ⬡ конфликт · ◖◗ инсайт</span>
    </div>
  )
}
