import { useEffect, useState } from 'react'
import { ArrowLeft, Check, LogOut, Undo2, UserPlus, X } from 'lucide-react'
import type { Agent, RolePreset } from '../types'
import { glyphOf } from '../types'
import { api } from '../api'
import { ModelPicker, type ModelInfo } from './ModelPicker'

interface Props {
  open: boolean
  agents: Agent[]
  roomModel: string
  onClose: () => void
  onCreate: (body: Parameters<typeof api.createAgent>[0]) => Promise<unknown>
  onPatch: (id: string, body: Parameters<typeof api.patchAgent>[1]) => Promise<unknown>
  onDm?: (id: string) => void
  dmUnread?: Record<string, number>
}

const PALETTE = ['#CCFF00', '#00F0FF', '#FF0055', '#f0abfc', '#fbbf24', '#c084fc', '#fb7185', '#fdba74', '#a3e635', '#7dd3fc', '#86efac', '#fde68a', '#67e8f9', '#f9a8d4', '#5eead4', '#c9c9d1']
const VOICES = ['alloy', 'ash', 'ballad', 'coral', 'echo', 'fable', 'nova', 'onyx', 'sage', 'shimmer']

type Draft = { name: string; archetype: string; system_core: string; color: string; glyph: string; model: string; voice: string; agenda: string }

const inp = 'w-full rounded border border-line bg-panel px-2 py-1 text-xs text-zinc-100 placeholder:text-zinc-600 focus:border-acid focus:outline-none'

/** «Двор»: who lives here. Invite from a role preset or write your own; edit character, model, voice; send away / bring back. */
export function YardModal({ open, agents, roomModel, onClose, onCreate, onPatch, onDm, dmUnread = {} }: Props) {
  const [tab, setTab] = useState<'cast' | 'invite'>('cast')
  const [roles, setRoles] = useState<RolePreset[]>([])
  const [models, setModels] = useState<ModelInfo[]>([])
  const [catalog, setCatalog] = useState<'loading' | 'ready' | 'error'>('loading')
  const [editing, setEditing] = useState<string | null>(null)
  const [preset, setPreset] = useState<RolePreset | 'custom' | null>(null)
  const [draft, setDraft] = useState<Draft | null>(null)
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState<string | null>(null)

  useEffect(() => {
    if (!open) return
    setTab('cast')
    setEditing(null)
    setPreset(null)
    setDraft(null)
    setErr(null)
    if (!roles.length) void api.roles().then(setRoles).catch(() => undefined)
    if (!models.length) {
      fetch('/api/models')
        .then((r) => (r.ok ? r.json() : Promise.reject(new Error(String(r.status)))))
        .then((list: ModelInfo[]) => {
          setModels(list)
          setCatalog('ready')
        })
        .catch(() => setCatalog('error'))
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  if (!open) return null
  const active = agents.filter((a) => a.active !== 0)
  const away = agents.filter((a) => a.active === 0)
  const taken = new Set(agents.map((a) => a.role || a.id))

  const startEdit = (a: Agent) => {
    setEditing(a.id)
    setDraft({ name: a.name, archetype: a.archetype, system_core: a.system_core, color: a.color, glyph: glyphOf(a), model: a.model, voice: a.voice, agenda: a.current_agenda })
    setErr(null)
  }
  const startInvite = (p: RolePreset | 'custom') => {
    setPreset(p)
    setDraft(
      p === 'custom'
        ? { name: '', archetype: '', system_core: '', color: PALETTE[(agents.length * 5) % PALETTE.length], glyph: '', model: '', voice: '', agenda: '' }
        : { name: p.name, archetype: p.archetype, system_core: p.core, color: p.color, glyph: p.glyph, model: '', voice: '', agenda: p.agenda },
    )
    setErr(null)
  }
  const run = async (fn: () => Promise<unknown>, after: () => void) => {
    setBusy(true)
    setErr(null)
    try {
      await fn()
      after()
    } catch (e) {
      setErr((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  const form = draft && (
    <div className="space-y-3">
      <div className="grid grid-cols-[1fr_72px] gap-2">
        <label className="block">
          <div className="mb-0.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">имя</div>
          <input value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} className={inp} placeholder="как зовут" />
        </label>
        <label className="block">
          <div className="mb-0.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">знак</div>
          <input value={draft.glyph} onChange={(e) => setDraft({ ...draft, glyph: e.target.value.slice(0, 2) })} className={`${inp} text-center text-base`} placeholder="Σ" />
        </label>
      </div>
      <label className="block">
        <div className="mb-0.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">кто это одной строкой</div>
        <input value={draft.archetype} onChange={(e) => setDraft({ ...draft, archetype: e.target.value })} className={inp} placeholder="Продюсер проектов, 33 года, дедлайны и «что отрезаем»." />
      </label>
      <label className="block">
        <div className="mb-0.5 flex items-baseline justify-between font-mono text-[10px] uppercase tracking-widest text-zinc-500">
          <span>характер (system prompt персонажа)</span>
          <span className="normal-case tracking-normal text-zinc-600">{draft.system_core.length} симв.</span>
        </div>
        <textarea
          value={draft.system_core}
          onChange={(e) => setDraft({ ...draft, system_core: e.target.value })}
          rows={9}
          className={`${inp} resize-y font-mono leading-relaxed`}
          placeholder="Ты — …, N лет, … Говоришь … Тебя кормят (дофамин): … Тебя убивают: …"
        />
        <div className="mt-0.5 text-[10px] text-zinc-600">Пиши во втором лице: кто, возраст, как говорит, что любит и что бесит. Чем конкретнее речевые привычки, тем живее.</div>
      </label>
      <label className="block">
        <div className="mb-0.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">с чего начинает (скрытая цель)</div>
        <input value={draft.agenda} onChange={(e) => setDraft({ ...draft, agenda: e.target.value })} className={inp} placeholder="Понять, чего игрок хочет на самом деле." />
      </label>
      <div>
        <div className="mb-0.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">цвет</div>
        <div className="flex flex-wrap items-center gap-1">
          {PALETTE.map((c) => (
            <button key={c} onClick={() => setDraft({ ...draft, color: c })} className={`h-5 w-5 rounded-full border-2 ${draft.color === c ? 'border-white' : 'border-transparent'}`} style={{ background: c }} aria-label={c} />
          ))}
        </div>
      </div>
      <div>
        <div className="mb-0.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">модель (пусто — общая: {roomModel})</div>
        <ModelPicker value={draft.model} onChange={(m) => setDraft({ ...draft, model: m })} models={models} status={catalog} placeholder="как у всех" />
        {draft.model && (
          <button onClick={() => setDraft({ ...draft, model: '' })} className="mt-0.5 font-mono text-[10px] text-zinc-500 hover:text-magenta">
            сбросить на общую
          </button>
        )}
      </div>
      <label className="block">
        <div className="mb-0.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">голос (TTS, если настроен сервер)</div>
        <div className="flex flex-wrap gap-1">
          {VOICES.map((v) => (
            <button key={v} onClick={() => setDraft({ ...draft, voice: draft.voice === v ? '' : v })} className={`rounded border px-1.5 py-0.5 font-mono text-[10px] ${draft.voice === v ? 'border-acid text-acid' : 'border-line text-zinc-500 hover:text-zinc-200'}`}>
              {v}
            </button>
          ))}
          <input value={draft.voice} onChange={(e) => setDraft({ ...draft, voice: e.target.value })} className={`${inp} w-28`} placeholder="своё имя голоса" />
        </div>
      </label>
      {err && <div className="rounded border border-magenta/50 bg-magenta/10 px-3 py-2 text-xs text-magenta">{err}</div>}
    </div>
  )

  return (
    <div className="fixed inset-0 z-50 flex items-end justify-center bg-black/70 sm:items-center sm:p-4" onClick={onClose}>
      <div className="flex h-[92vh] w-full max-w-3xl flex-col overflow-hidden rounded-t-xl border border-line bg-void shadow-2xl sm:h-[84vh] sm:rounded-xl" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center gap-2 border-b border-line px-4 py-2">
          <h2 className="font-mono text-sm font-bold tracking-widest text-zinc-100">ДВОР</h2>
          <span className="font-mono text-[10px] text-zinc-500">
            {active.length} {active.length === 1 ? 'житель' : active.length < 5 ? 'жителя' : 'жителей'}
            {away.length ? ` · ${away.length} в отъезде` : ''} · до 8
          </span>
          <div className="ml-auto flex items-center gap-1">
            {(['cast', 'invite'] as const).map((t) => (
              <button
                key={t}
                onClick={() => {
                  setTab(t)
                  setEditing(null)
                  setPreset(null)
                  setDraft(null)
                }}
                className={`rounded px-2 py-1 font-mono text-[11px] ${tab === t ? 'bg-zinc-800 text-zinc-100' : 'text-zinc-500 hover:text-zinc-200'}`}
              >
                {t === 'cast' ? 'жители' : 'позвать'}
              </button>
            ))}
            <button onClick={onClose} className="rounded p-1 text-zinc-500 hover:text-zinc-200" aria-label="Закрыть">
              <X size={16} />
            </button>
          </div>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto p-4">
          {tab === 'cast' && !editing && (
            <div className="space-y-4">
              <div className="grid gap-2 sm:grid-cols-2">
                {active.map((a) => (
                  <div key={a.id} className="flex gap-2 rounded border border-line bg-panel/60 p-2">
                    <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded border text-lg font-bold" style={{ borderColor: a.color, color: a.color }}>
                      {a.hp <= 0 ? '☠' : glyphOf(a)}
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-baseline gap-2">
                        <span className="truncate text-sm font-semibold text-zinc-100">{a.name}</span>
                        <span className="font-mono text-[9px] text-zinc-600">{a.role && a.role !== 'custom' ? roles.find((r) => r.id === a.role)?.role ?? a.role : 'свой'}</span>
                      </div>
                      <div className="line-clamp-2 text-[11px] text-zinc-400">{a.archetype}</div>
                      <div className="mt-1 flex flex-wrap items-center gap-x-3 font-mono text-[9px] text-zinc-600">
                        <span>{a.model ? `модель ${a.model}` : 'общая модель'}</span>
                        {a.voice && <span>голос {a.voice}</span>}
                        <span>HP {a.hp} · доф {a.dopamine}</span>
                      </div>
                      <div className="mt-1.5 flex gap-1.5">
                        <button onClick={() => startEdit(a)} className="rounded border border-line px-2 py-0.5 font-mono text-[10px] text-zinc-300 hover:border-acid hover:text-acid">
                          настроить
                        </button>
                        {onDm && a.hp > 0 && (
                          <button onClick={() => onDm(a.id)} className="rounded border border-line px-2 py-0.5 font-mono text-[10px] text-zinc-300 hover:border-acid hover:text-acid" title="Личные сообщения">
                            ✉ лично{(dmUnread[a.id] ?? 0) > 0 ? ` · ${dmUnread[a.id]}` : ''}
                          </button>
                        )}
                        {a.hp <= 0 && (
                          <button onClick={() => void run(() => onPatch(a.id, { revive: true }), () => undefined)} disabled={busy} className="rounded border border-acid/50 px-2 py-0.5 font-mono text-[10px] text-acid hover:bg-acid/10">
                            вернуть к жизни
                          </button>
                        )}
                        <button
                          onClick={() => {
                            if (confirm(`${a.name} уезжает из двора? История и дневник останутся, можно вернуть.`)) void run(() => onPatch(a.id, { active: false }), () => undefined)
                          }}
                          disabled={busy || active.length <= 1}
                          className="ml-auto flex items-center gap-1 rounded border border-line px-2 py-0.5 font-mono text-[10px] text-zinc-500 hover:border-magenta hover:text-magenta disabled:opacity-40"
                          title="Отправить в отъезд"
                        >
                          <LogOut size={10} /> уехать
                        </button>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
              {away.length > 0 && (
                <div>
                  <div className="mb-1 font-mono text-[10px] uppercase tracking-widest text-zinc-500">в отъезде</div>
                  <div className="flex flex-wrap gap-1.5">
                    {away.map((a) => (
                      <button key={a.id} onClick={() => void run(() => onPatch(a.id, { active: true }), () => undefined)} disabled={busy || active.length >= 8} className="flex items-center gap-1.5 rounded border border-line px-2 py-1 font-mono text-[11px] text-zinc-400 hover:border-acid hover:text-acid disabled:opacity-40" title="Вернуть во двор">
                        <span style={{ color: a.color }}>{glyphOf(a)}</span> {a.name} <Undo2 size={10} />
                      </button>
                    ))}
                  </div>
                </div>
              )}
              <button onClick={() => setTab('invite')} className="flex items-center gap-2 rounded border border-dashed border-acid/50 px-3 py-2 font-mono text-xs text-acid hover:bg-acid/10">
                <UserPlus size={13} /> позвать во двор
              </button>
              {err && <div className="rounded border border-magenta/50 bg-magenta/10 px-3 py-2 text-xs text-magenta">{err}</div>}
            </div>
          )}

          {tab === 'cast' && editing && draft && (
            <div className="space-y-3">
              <button onClick={() => setEditing(null)} className="flex items-center gap-1 font-mono text-[11px] text-zinc-500 hover:text-zinc-200">
                <ArrowLeft size={12} /> к жителям
              </button>
              {form}
              <div className="flex gap-2 border-t border-line pt-3">
                <button
                  onClick={() => void run(() => onPatch(editing, { ...draft }), () => setEditing(null))}
                  disabled={busy || draft.name.trim().length < 2 || draft.system_core.trim().length < 40}
                  className="flex items-center gap-1 rounded bg-acid px-4 py-1.5 font-mono text-xs font-bold text-void hover:shadow-glow disabled:opacity-40"
                >
                  <Check size={12} /> сохранить
                </button>
                <span className="self-center font-mono text-[10px] text-zinc-600">Характер меняется со следующей реплики; HP, дофамин и память остаются.</span>
              </div>
            </div>
          )}

          {tab === 'invite' && !draft && (
            <div className="space-y-3">
              <div className="text-[11px] text-zinc-500">Выбери роль — имя, характер и первый проект уже написаны, всё можно поправить. Или опиши своего.</div>
              <div className="grid gap-2 sm:grid-cols-2">
                {roles.map((p) => {
                  const here = taken.has(p.id)
                  return (
                    <button key={p.id} onClick={() => startInvite(p)} className={`flex gap-2 rounded border p-2 text-left transition hover:border-acid ${here ? 'border-line/60 opacity-60' : 'border-line bg-panel/60'}`}>
                      <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded border text-lg font-bold" style={{ borderColor: p.color, color: p.color }}>
                        {p.glyph}
                      </div>
                      <div className="min-w-0">
                        <div className="flex items-baseline gap-2">
                          <span className="text-sm font-semibold text-zinc-100">{p.role}</span>
                          <span className="truncate font-mono text-[9px] text-zinc-600">{p.name}</span>
                        </div>
                        <div className="text-[11px] text-zinc-400">{p.pitch}</div>
                        {here && <div className="font-mono text-[9px] text-zinc-600">уже во дворе — будет второй такой</div>}
                      </div>
                    </button>
                  )
                })}
                <button onClick={() => startInvite('custom')} className="flex items-center justify-center gap-2 rounded border border-dashed border-line p-3 font-mono text-xs text-zinc-400 hover:border-acid hover:text-acid">
                  <UserPlus size={14} /> свой персонаж с нуля
                </button>
              </div>
            </div>
          )}

          {tab === 'invite' && draft && (
            <div className="space-y-3">
              <button
                onClick={() => {
                  setDraft(null)
                  setPreset(null)
                }}
                className="flex items-center gap-1 font-mono text-[11px] text-zinc-500 hover:text-zinc-200"
              >
                <ArrowLeft size={12} /> к ролям
              </button>
              {form}
              <div className="flex gap-2 border-t border-line pt-3">
                <button
                  onClick={() =>
                    void run(
                      () => onCreate({ role: preset && preset !== 'custom' ? preset.id : undefined, ...draft }),
                      () => {
                        setDraft(null)
                        setPreset(null)
                        setTab('cast')
                      },
                    )
                  }
                  disabled={busy || active.length >= 8 || draft.name.trim().length < 2 || draft.system_core.trim().length < 40}
                  className="flex items-center gap-1 rounded bg-acid px-4 py-1.5 font-mono text-xs font-bold text-void hover:shadow-glow disabled:opacity-40"
                >
                  <UserPlus size={12} /> позвать
                </button>
                <span className="self-center font-mono text-[10px] text-zinc-600">Появится в чате, получит свой проект в библиотеке и нейтральные отношения со всеми.</span>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
