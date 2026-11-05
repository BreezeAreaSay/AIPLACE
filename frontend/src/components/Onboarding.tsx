import { useEffect, useState } from 'react'
import { ArrowRight, Check, KeyRound, MapPin, Users, X } from 'lucide-react'
import type { Agent, RolePreset } from '../types'
import { api } from '../api'
import { ModelPicker, type ModelInfo } from './ModelPicker'

interface Props {
  open: boolean
  agents: Agent[]
  hasKey: boolean
  model: string
  onClose: () => void
  onDone: () => void
  onCreateAgent: (body: { role: string }) => Promise<unknown>
  onPatchAgent: (id: string, body: { active?: boolean }) => Promise<unknown>
}

const DEFAULT_CAST = ['physicist', 'lit_teacher', 'graffiti']
const inp = 'w-full rounded border border-line bg-panel px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-600 focus:border-acid focus:outline-none'

/** First run: key and model → who you are → who lives in the yard. Every step can be skipped. */
export function Onboarding({ open, agents, hasKey, model: roomModel, onClose, onDone, onCreateAgent, onPatchAgent }: Props) {
  const [step, setStep] = useState(0)
  const [key, setKey] = useState('')
  const [model, setModel] = useState(roomModel)
  const [models, setModels] = useState<ModelInfo[]>([])
  const [catalog, setCatalog] = useState<'loading' | 'ready' | 'error'>('loading')
  const [name, setName] = useState('')
  const [bio, setBio] = useState('')
  const [city, setCity] = useState('')
  const [roles, setRoles] = useState<RolePreset[]>([])
  const [picked, setPicked] = useState<Set<string>>(new Set(DEFAULT_CAST))
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState<string | null>(null)

  useEffect(() => {
    if (!open) return
    setStep(0)
    setErr(null)
    void api.roles().then(setRoles).catch(() => undefined)
    void fetch('/api/models')
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(String(r.status)))))
      .then((list: ModelInfo[]) => {
        setModels(list)
        setCatalog('ready')
      })
      .catch(() => setCatalog('error'))
    void api
      .settings()
      .then((v) => {
        const g = (k: string) => v.integrations.find((i) => i.key === k)?.value ?? ''
        setName(g('PLAYER_NAME'))
        setBio(g('PLAYER_BIO'))
        setCity(g('WEATHER_CITY'))
      })
      .catch(() => undefined)
  }, [open])

  if (!open) return null

  const finish = async () => {
    setBusy(true)
    setErr(null)
    try {
      const integrations: Record<string, string> = { PLAYER_NAME: name.trim(), PLAYER_BIO: bio.trim(), WEATHER_CITY: city.trim() }
      await api.saveSettings({ api_key: key.trim() || undefined, model: model.trim() || undefined, integrations })
      const have = new Set(agents.map((a) => a.role || a.id))
      for (const r of picked) if (!have.has(r)) await onCreateAgent({ role: r })
      for (const a of agents) {
        const role = a.role || a.id
        if (a.active !== 0 && !picked.has(role) && DEFAULT_CAST.includes(role)) await onPatchAgent(a.id, { active: false })
        if (a.active === 0 && picked.has(role)) await onPatchAgent(a.id, { active: true })
      }
      try {
        localStorage.setItem('triad.onboarded', '1')
      } catch {
        /* ignore */
      }
      onDone()
    } catch (e) {
      setErr((e as Error).message)
    } finally {
      setBusy(false)
    }
  }
  const skip = () => {
    try {
      localStorage.setItem('triad.onboarded', '1')
    } catch {
      /* ignore */
    }
    onClose()
  }

  const steps = [
    { icon: KeyRound, title: 'Мозг' },
    { icon: MapPin, title: 'Ты' },
    { icon: Users, title: 'Двор' },
  ]

  return (
    <div className="fixed inset-0 z-[55] flex items-end justify-center bg-black/80 sm:items-center sm:p-4">
      <div className="flex max-h-[94vh] w-full max-w-xl flex-col overflow-hidden rounded-t-xl border border-line bg-void shadow-2xl sm:rounded-xl">
        <div className="flex items-center gap-3 border-b border-line px-4 py-3">
          <div className="font-mono text-xs tracking-[0.3em] text-acid">ПЕРВЫЙ ЗАПУСК</div>
          <div className="flex items-center gap-1 font-mono text-[10px] text-zinc-500">
            {steps.map((s, i) => (
              <span key={s.title} className={`rounded px-1.5 py-0.5 ${i === step ? 'bg-zinc-800 text-zinc-100' : i < step ? 'text-acid' : ''}`}>
                {i + 1}. {s.title}
              </span>
            ))}
          </div>
          <button onClick={skip} className="ml-auto rounded p-1 text-zinc-500 hover:text-zinc-200" title="Пропустить (можно вернуться из настроек)">
            <X size={16} />
          </button>
        </div>

        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-4 py-4 text-sm">
          {step === 0 && (
            <>
              <p className="text-zinc-300">
                Двор думает через OpenRouter: один ключ, любые модели, есть бесплатные. Без ключа всё работает в mock-режиме с шаблонными репликами, чтобы потрогать интерфейс.
              </p>
              <label className="block">
                <div className="mb-1 flex items-baseline justify-between font-mono text-[10px] uppercase tracking-widest text-zinc-500">
                  <span>OpenRouter API key</span>
                  <span className={hasKey ? 'text-acid' : 'text-zinc-600'}>{hasKey ? 'уже есть' : 'нет'}</span>
                </div>
                <input type="password" autoComplete="off" value={key} onChange={(e) => setKey(e.target.value)} placeholder={hasKey ? 'оставь пустым, чтобы не менять' : 'sk-or-v1-…'} className={`${inp} font-mono text-xs`} />
                <div className="mt-1 text-[11px] text-zinc-500">
                  Взять:{' '}
                  <a href="https://openrouter.ai/keys" target="_blank" rel="noreferrer" className="text-cyan underline">
                    openrouter.ai/keys
                  </a>
                  . Хранится только на этой машине.
                </div>
              </label>
              <div>
                <div className="mb-1 font-mono text-[10px] uppercase tracking-widest text-zinc-500">модель персонажей</div>
                <ModelPicker value={model} onChange={setModel} models={models} status={catalog} />
                <div className="mt-1 text-[11px] text-zinc-500">Потом каждому жителю можно дать свою. Бесплатные (:free) работают, но с лимитами.</div>
              </div>
            </>
          )}
          {step === 1 && (
            <>
              <p className="text-zinc-300">Жители будут знать, с кем говорят. Это попадает только в их промпт, никуда больше.</p>
              <label className="block">
                <div className="mb-1 font-mono text-[10px] uppercase tracking-widest text-zinc-500">как к тебе обращаться</div>
                <input value={name} onChange={(e) => setName(e.target.value)} placeholder="Саша" className={inp} />
              </label>
              <label className="block">
                <div className="mb-1 font-mono text-[10px] uppercase tracking-widest text-zinc-500">о себе (пара строк)</div>
                <textarea value={bio} onChange={(e) => setBio(e.target.value)} rows={3} placeholder="Чем занимаешься, что сейчас важно, чего хочешь от этого двора: пинков, разговоров, памяти, идей…" className={`${inp} resize-y`} />
              </label>
              <label className="block">
                <div className="mb-1 font-mono text-[10px] uppercase tracking-widest text-zinc-500">город (погода за окном, по желанию)</div>
                <input value={city} onChange={(e) => setCity(e.target.value)} placeholder="Москва" className={inp} />
              </label>
            </>
          )}
          {step === 2 && (
            <>
              <p className="text-zinc-300">Кто живёт во дворе. Трое отмечены по умолчанию; любой состав можно поменять потом кнопкой «двор».</p>
              <div className="grid gap-1.5 sm:grid-cols-2">
                {roles.map((r) => {
                  const on = picked.has(r.id)
                  return (
                    <button
                      key={r.id}
                      onClick={() => {
                        const n = new Set(picked)
                        if (on) n.delete(r.id)
                        else if (n.size < 8) n.add(r.id)
                        setPicked(n)
                      }}
                      className={`flex gap-2 rounded border p-2 text-left transition ${on ? 'border-acid/70 bg-acid/5' : 'border-line hover:border-zinc-500'}`}
                    >
                      <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded border text-base font-bold" style={{ borderColor: r.color, color: r.color }}>
                        {r.glyph}
                      </div>
                      <div className="min-w-0">
                        <div className="flex items-center gap-1 text-xs font-semibold text-zinc-100">
                          {r.role} {on && <Check size={11} className="text-acid" />}
                        </div>
                        <div className="line-clamp-2 text-[10px] text-zinc-500">{r.pitch}</div>
                      </div>
                    </button>
                  )
                })}
              </div>
              <div className="font-mono text-[10px] text-zinc-600">выбрано {picked.size} из 8</div>
            </>
          )}
          {err && <div className="rounded border border-magenta/50 bg-magenta/10 px-3 py-2 text-xs text-magenta">{err}</div>}
        </div>

        <div className="flex items-center gap-2 border-t border-line px-4 py-3">
          {step > 0 && (
            <button onClick={() => setStep(step - 1)} className="rounded border border-line px-3 py-1.5 font-mono text-xs text-zinc-400 hover:text-zinc-200">
              назад
            </button>
          )}
          <button onClick={skip} className="font-mono text-[11px] text-zinc-600 hover:text-zinc-300">
            пропустить
          </button>
          {step < 2 ? (
            <button onClick={() => setStep(step + 1)} className="ml-auto flex items-center gap-1 rounded bg-acid px-4 py-1.5 font-mono text-xs font-bold text-void hover:shadow-glow">
              дальше <ArrowRight size={12} />
            </button>
          ) : (
            <button onClick={() => void finish()} disabled={busy || picked.size === 0} className="ml-auto flex items-center gap-1 rounded bg-acid px-4 py-1.5 font-mono text-xs font-bold text-void hover:shadow-glow disabled:opacity-40">
              {busy ? '…' : 'во двор'} <ArrowRight size={12} />
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
