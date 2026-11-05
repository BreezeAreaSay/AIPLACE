import { useEffect, useState } from 'react'
import { ArrowUpCircle, KeyRound, Lock, X } from 'lucide-react'
import { api, API_BASE } from '../api'
import type { SettingsView } from '../types'
import { ModelPicker, type ModelInfo } from './ModelPicker'

interface Props {
  open: boolean
  onClose: () => void
  onSaved: () => void
}

export function SettingsModal({ open, onClose, onSaved }: Props) {
  const [view, setView] = useState<SettingsView | null>(null)
  const [key, setKey] = useState('')
  const [model, setModel] = useState('')
  const [director, setDirector] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [ok, setOk] = useState<string | null>(null)
  const [models, setModels] = useState<ModelInfo[]>([])
  const [catalog, setCatalog] = useState<'loading' | 'ready' | 'error'>('loading')

  useEffect(() => {
    if (!open || models.length) return
    setCatalog('loading')
    fetch(API_BASE + '/api/models')
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(String(r.status)))))
      .then((list: ModelInfo[]) => {
        setModels(list)
        setCatalog('ready')
      })
      .catch(() => setCatalog('error'))
  }, [open, models.length])

  useEffect(() => {
    if (!open) return
    setError(null)
    setOk(null)
    setKey('')
    api
      .settings()
      .then((v) => {
        setView(v)
        setModel(v.model)
        setDirector(v.director_model)
      })
      .catch((e) => setError((e as Error).message))
  }, [open])

  if (!open) return null

  const save = async (clearKey = false) => {
    setBusy(true)
    setError(null)
    setOk(null)
    try {
      const v = await api.saveSettings({
        api_key: key.trim() || undefined,
        clear_key: clearKey,
        model: model.trim() || undefined,
        director_model: director.trim() || undefined,
      })
      setView(v)
      setKey('')
      setOk(clearKey ? 'Ключ удалён — комната в mock-режиме.' : 'Сохранено. Следующий ход пойдёт через OpenRouter.')
      onSaved()
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  const editable = view?.editable ?? false

  return (
    <div className="fixed inset-0 z-50 flex items-end justify-center bg-black/70 p-0 sm:items-center sm:p-4" onClick={onClose}>
      <div
        className="max-h-[92vh] w-full max-w-lg overflow-y-auto rounded-t-xl border border-line bg-void p-4 shadow-2xl sm:rounded-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-3 flex items-center gap-2">
          <KeyRound size={16} className="text-acid" />
          <h2 className="font-mono text-sm font-bold tracking-widest text-zinc-100">НАСТРОЙКИ LLM</h2>
          <button onClick={onClose} className="ml-auto rounded p-1 text-zinc-500 hover:text-zinc-200" aria-label="Закрыть">
            <X size={16} />
          </button>
        </div>

        {!view && !error && <div className="font-mono text-xs text-zinc-500">загружаю…</div>}

        {view && (
          <div className="space-y-4 text-sm">
            {!editable && (
              <div className="flex items-start gap-2 rounded border border-magenta/40 bg-magenta/10 px-3 py-2 text-xs text-magenta">
                <Lock size={14} className="mt-0.5 shrink-0" />
                <span>
                  Этот клиент не с той машины, где запущен сервер — менять ключ отсюда нельзя. На сервере:{' '}
                  <code className="text-zinc-200">triad setup</code> или <code className="text-zinc-200">triad key sk-or-…</code>. Чтобы
                  разрешить удалённо — <code className="text-zinc-200">ALLOW_REMOTE_SETTINGS=true</code>.
                </span>
              </div>
            )}

            <label className="block">
              <div className="mb-1 flex items-baseline justify-between font-mono text-[10px] uppercase tracking-widest text-zinc-500">
                <span>OpenRouter API key</span>
                <span className={view.has_key ? 'text-acid' : 'text-magenta'}>{view.has_key ? `установлен · ${view.key_hint}` : 'нет → mock-режим'}</span>
              </div>
              <input
                type="password"
                autoComplete="off"
                value={key}
                disabled={!editable}
                onChange={(e) => setKey(e.target.value)}
                placeholder={view.has_key ? 'вставь новый, чтобы заменить' : 'sk-or-v1-…'}
                className="w-full rounded border border-line bg-panel px-3 py-2 font-mono text-xs text-zinc-100 placeholder:text-zinc-600 focus:border-acid focus:outline-none disabled:opacity-50"
              />
              <div className="mt-1 text-[11px] text-zinc-500">
                Взять ключ:{' '}
                <a href="https://openrouter.ai/keys" target="_blank" rel="noreferrer" className="text-cyan underline">
                  openrouter.ai/keys
                </a>
                . Хранится только в <code className="text-zinc-400">{view.config_path}</code>, в браузер не возвращается.
              </div>
            </label>

            <div>
              <div className="mb-1 font-mono text-[10px] uppercase tracking-widest text-zinc-500">Модель персонажей</div>
              <ModelPicker value={model} onChange={setModel} models={models} status={catalog} disabled={!editable} />
              <div className="mt-1 text-[11px] text-zinc-500">Держит характер. Бесплатные (:free) работают, но с лимитами запросов — в режиме субагентов на раунд уходит 6–10 вызовов.</div>
            </div>
            <div>
              <div className="mb-1 font-mono text-[10px] uppercase tracking-widest text-zinc-500">Модель Режиссёра и субагентов</div>
              <ModelPicker value={director} onChange={setDirector} models={models} status={catalog} disabled={!editable} preferJson />
              <div className="mt-1 text-[11px] text-zinc-500">
                Reader / Caster / Картограф / Психолог — маленькие JSON-задачи, подойдёт быстрая. Режимы: Режиссёр <code className="text-zinc-300">{view.director_mode}</code>, агенты{' '}
                <code className="text-zinc-300">{view.agent_mode}</code> (DIRECTOR_MODE / AGENT_MODE в config.env).
              </div>
            </div>

            {error && <div className="rounded border border-magenta/50 bg-magenta/10 px-3 py-2 text-xs text-magenta">{error}</div>}
            {ok && <div className="rounded border border-acid/50 bg-acid/10 px-3 py-2 text-xs text-acid">{ok}</div>}

            <div className="flex flex-wrap gap-2">
              <button
                onClick={() => void save(false)}
                disabled={!editable || busy}
                className="rounded bg-acid px-4 py-2 font-mono text-xs font-bold text-void hover:shadow-glow disabled:opacity-40"
              >
                {busy ? '…' : 'Сохранить'}
              </button>
              {view.has_key && (
                <button
                  onClick={() => {
                    if (confirm('Удалить ключ? Комната перейдёт в mock-режим.')) void save(true)
                  }}
                  disabled={!editable || busy}
                  className="rounded border border-magenta/50 px-3 py-2 font-mono text-xs text-magenta hover:bg-magenta/10 disabled:opacity-40"
                >
                  удалить ключ
                </button>
              )}
            </div>

            <div className="border-t border-line pt-3 font-mono text-[11px] text-zinc-500">
              <div className="flex items-center gap-2">
                <span>
                  triad v{view.version} · UI {view.ui_embedded ? 'встроен' : 'внешний (dev)'}
                </span>
                {view.update && (
                  <a href={view.update.url} target="_blank" rel="noreferrer" className="flex items-center gap-1 text-acid underline">
                    <ArrowUpCircle size={12} /> доступна v{view.update.latest}
                  </a>
                )}
              </div>
              {view.update && (
                <div className="mt-1">
                  Обновить: <code className="text-zinc-300">triad update</code> (или заново{' '}
                  <code className="text-zinc-300">curl -fsSL https://raw.githubusercontent.com/BreezeAreaSay/AIPLACE/main/install.sh | sh</code>)
                </div>
              )}
            </div>
          </div>
        )}
        {!view && error && <div className="rounded border border-magenta/50 bg-magenta/10 px-3 py-2 text-xs text-magenta">{error}</div>}
      </div>
    </div>
  )
}
