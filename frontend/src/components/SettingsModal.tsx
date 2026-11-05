import { useEffect, useState } from 'react'
import { ArrowUpCircle, Copy, Download, Globe, GitBranch, KeyRound, Lock, RefreshCw, Upload, Wand2, X } from 'lucide-react'
import { api, API_BASE } from '../api'
import type { SettingsView, SnapshotInfo, SourceUpdate } from '../types'
import { ModelPicker, type ModelInfo } from './ModelPicker'

interface Props {
  open: boolean
  onClose: () => void
  onSaved: () => void
  sourceUpdate: SourceUpdate | null
  restorePending: string | null
  onWizard: () => void
  repoDir: string | null
  exePath: string
}

const GROUPS: { title: string; note: string; keys: string[] }[] = [
  { title: 'Обновление', note: 'нужно только если exe лежит не внутри клона', keys: ['TRIAD_REPO'] },
  { title: 'Поиск в сети', note: 'ddg работает без ключа; Tavily/Brave точнее', keys: ['SEARCH_PROVIDER', 'SEARCH_API_KEY', 'SEARXNG_URL'] },
  { title: 'Погода и календарь', note: 'Open-Meteo без ключа; .ics — приватная ссылка твоего календаря', keys: ['WEATHER_CITY', 'CALENDAR_ICS_URL'] },
  { title: 'Справочники', note: 'книги — OpenLibrary без ключа, Википедия без ключа, фильмы — TMDB', keys: ['TMDB_API_KEY'] },
  { title: 'GitHub', note: 'публичные репо без токена', keys: ['GITHUB_TOKEN'] },
  { title: 'Голос', note: 'без ключей — голос браузера; OpenAI/Groq/локальный сервер — по ключу', keys: ['TTS_URL', 'TTS_API_KEY', 'TTS_MODEL', 'TTS_VOICES', 'STT_URL', 'STT_API_KEY', 'STT_MODEL'] },
  { title: 'Ленты', note: 'объекты типа «Лента» опрашиваются сами', keys: ['FEEDS_EVERY_MIN'] },
]

export function SettingsModal({ open, onClose, onSaved, sourceUpdate, restorePending, onWizard, repoDir, exePath }: Props) {
  const [restoreMsg, setRestoreMsg] = useState<string | null>(null)
  const [snaps, setSnaps] = useState<SnapshotInfo[]>([])
  const loadSnaps = () => void api.snapshots().then(setSnaps).catch(() => setSnaps([]))
  useEffect(() => {
    if (open) loadSnaps()
  }, [open])
  const [su, setSu] = useState<SourceUpdate | null>(sourceUpdate)
  const [suBusy, setSuBusy] = useState<'idle' | 'check' | 'run' | 'gone'>('idle')
  const [suErr, setSuErr] = useState<string | null>(null)
  useEffect(() => setSu(sourceUpdate), [sourceUpdate])
  const [view, setView] = useState<SettingsView | null>(null)
  const [key, setKey] = useState('')
  const [model, setModel] = useState('')
  const [director, setDirector] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [ok, setOk] = useState<string | null>(null)
  const [models, setModels] = useState<ModelInfo[]>([])
  const [catalog, setCatalog] = useState<'loading' | 'ready' | 'error'>('loading')
  const [integ, setInteg] = useState<Record<string, string>>({})
  const [copied, setCopied] = useState(false)

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
        setInteg({})
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
        integrations: Object.keys(integ).length ? integ : undefined,
      })
      setView(v)
      setKey('')
      setInteg({})
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
        className="max-h-[92vh] w-full max-w-xl overflow-y-auto rounded-t-xl border border-line bg-void p-4 shadow-2xl sm:rounded-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-3 flex items-center gap-2">
          <KeyRound size={16} className="text-acid" />
          <h2 className="font-mono text-sm font-bold tracking-widest text-zinc-100">НАСТРОЙКИ</h2>
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
                Reader / Caster / Архивариус / Психолог — маленькие JSON-задачи, подойдёт быстрая. Режимы: Режиссёр <code className="text-zinc-300">{view.director_mode}</code>, агенты{' '}
                <code className="text-zinc-300">{view.agent_mode}</code>, инструменты <code className="text-zinc-300">{view.agent_tools}</code> (native tool calling, если модель умеет; иначе теги).
                Внешний MCP: <code className="text-zinc-300">POST /mcp</code>.
              </div>
            </div>

            <div className="border-t border-line pt-3">
              <div className="mb-2 flex items-center gap-2 font-mono text-[10px] uppercase tracking-widest text-zinc-500">
                <Globe size={12} className="text-acid" /> окна наружу
                <span className="ml-auto normal-case tracking-normal text-zinc-600">
                  {Object.entries(view.features)
                    .filter(([, on]) => on)
                    .map(([k]) => k)
                    .join(' · ') || 'ничего не включено'}
                </span>
              </div>
              <div className="space-y-3">
                {GROUPS.map((g) => (
                  <div key={g.title}>
                    <div className="mb-1 text-[11px] text-zinc-300">
                      {g.title} <span className="text-zinc-600">— {g.note}</span>
                    </div>
                    <div className="space-y-1">
                      {g.keys.map((k) => {
                        const it = view.integrations.find((i) => i.key === k)
                        if (!it) return null
                        const pending = integ[k]
                        return (
                          <div key={k} className="grid grid-cols-[150px_1fr_auto] items-center gap-2">
                            <span className="truncate font-mono text-[10px] text-zinc-500" title={it.hint}>
                              {k}
                            </span>
                            {k === 'SEARCH_PROVIDER' ? (
                              <select value={pending ?? it.value ?? ''} disabled={!editable} onChange={(e) => setInteg((m) => ({ ...m, [k]: e.target.value }))} className="rounded border border-line bg-panel px-2 py-1 font-mono text-xs text-zinc-100 focus:border-acid focus:outline-none">
                                <option value="">ddg (без ключа)</option>
                                <option value="tavily">tavily</option>
                                <option value="brave">brave</option>
                                <option value="searxng">searxng</option>
                                <option value="off">выключить</option>
                              </select>
                            ) : (
                              <input
                                type={it.secret ? 'password' : 'text'}
                                autoComplete="off"
                                value={pending ?? (it.secret ? '' : it.value)}
                                disabled={!editable}
                                onChange={(e) => setInteg((m) => ({ ...m, [k]: e.target.value }))}
                                placeholder={it.secret && it.set ? `установлен · ${it.value}` : it.hint}
                                className="rounded border border-line bg-panel px-2 py-1 font-mono text-xs text-zinc-100 placeholder:text-zinc-600 focus:border-acid focus:outline-none disabled:opacity-50"
                              />
                            )}
                            {it.set || pending ? (
                              <button onClick={() => setInteg((m) => ({ ...m, [k]: '' }))} disabled={!editable} className="text-zinc-600 hover:text-magenta disabled:opacity-40" title="Очистить">
                                <X size={12} />
                              </button>
                            ) : (
                              <span />
                            )}
                          </div>
                        )
                      })}
                    </div>
                  </div>
                ))}
                <div className="rounded border border-line/60 bg-panel/40 px-2 py-1.5 text-[11px] text-zinc-400">
                  Подписка на календарь библиотеки (сроки задач, события, слово): вставь ссылку в Google / Яндекс / Apple Calendar как «по URL».
                  <div className="mt-1 flex items-center gap-2">
                    <code className="truncate text-zinc-200">{api.calendarUrl()}</code>
                    <button
                      onClick={() => {
                        void navigator.clipboard.writeText(api.calendarUrl()).then(() => {
                          setCopied(true)
                          window.setTimeout(() => setCopied(false), 1200)
                        })
                      }}
                      className="shrink-0 text-zinc-500 hover:text-acid"
                      title="Скопировать"
                    >
                      <Copy size={12} />
                    </button>
                    {copied && <span className="text-acid">скопировано</span>}
                  </div>
                </div>
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

            <div className="border-t border-line pt-3">
              <div className="mb-1.5 flex items-center gap-2 font-mono text-[10px] uppercase tracking-widest text-zinc-500">
                <Download size={12} className="text-acid" /> бэкап комнаты
              </div>
              <div className="flex flex-wrap items-center gap-2 text-[11px]">
                <a href={api.backupUrl()} download className="flex items-center gap-1 rounded border border-line px-3 py-1.5 font-mono text-xs text-zinc-300 hover:border-acid hover:text-acid">
                  <Download size={12} /> скачать .zip
                </a>
                <label className={`flex cursor-pointer items-center gap-1 rounded border border-line px-3 py-1.5 font-mono text-xs text-zinc-300 hover:border-acid hover:text-acid ${!editable ? 'pointer-events-none opacity-40' : ''}`}>
                  <Upload size={12} /> восстановить из .zip
                  <input
                    type="file"
                    accept=".zip,application/zip"
                    className="hidden"
                    onChange={(e) => {
                      const f = e.target.files?.[0]
                      e.target.value = ''
                      if (!f) return
                      if (!confirm('Текущая комната будет заменена при следующем запуске (старая база останется рядом). Продолжить?')) return
                      void api
                        .restore(f)
                        .then((r) => {
                          setRestoreMsg(r.message)
                          onSaved()
                        })
                        .catch((err) => setRestoreMsg(`ошибка: ${(err as Error).message}`))
                    }}
                  />
                </label>
                <button onClick={onWizard} className="flex items-center gap-1 rounded border border-line px-3 py-1.5 font-mono text-xs text-zinc-300 hover:border-acid hover:text-acid">
                  <Wand2 size={12} /> мастер первого запуска
                </button>
              </div>
              {(restorePending || restoreMsg) && (
                <div className="mt-2 flex items-center gap-2 rounded border border-acid/40 bg-acid/5 px-3 py-2 text-[11px] text-acid">
                  <span>{restoreMsg ?? 'Бэкап подготовлен — перезапусти приложение, чтобы он применился.'}</span>
                  {restorePending && (
                    <button
                      onClick={() => {
                        void api.restoreCancel().then(() => {
                          setRestoreMsg(null)
                          onSaved()
                        })
                      }}
                      className="ml-auto underline"
                    >
                      отменить
                    </button>
                  )}
                </div>
              )}
              <div className="mt-1 text-[10px] text-zinc-600">В архиве база комнаты и все вложения. Положи в облако или перенеси на другую машину.</div>
              <div className="mt-2">
                <div className="mb-1 flex items-center gap-2 font-mono text-[10px] text-zinc-500">
                  снимки на этой машине · {snaps.length} <span className="text-zinc-600">(~/.triad/backups, авто раз в день, хранится 14)</span>
                  <button
                    onClick={() =>
                      void api
                        .createSnapshot()
                        .then(() => loadSnaps())
                        .catch((e) => setRestoreMsg(`ошибка: ${(e as Error).message}`))
                    }
                    className="ml-auto rounded border border-line px-2 py-0.5 text-[10px] text-zinc-300 hover:border-acid hover:text-acid"
                  >
                    + снимок сейчас
                  </button>
                </div>
                <div className="max-h-40 space-y-0.5 overflow-y-auto">
                  {snaps.slice(0, 20).map((sn) => (
                    <div key={sn.name} className="flex items-center gap-2 rounded border border-line/60 px-2 py-0.5 font-mono text-[10px] text-zinc-400">
                      <span className="truncate">{sn.name}</span>
                      <span className="shrink-0 text-zinc-600">{Math.round(sn.bytes / 1024)} КБ</span>
                      <button
                        onClick={() => {
                          if (!confirm(`Откатить комнату к снимку ${sn.name}? Применится при следующем запуске, текущая база останется рядом.`)) return
                          void api
                            .restoreSnapshot(sn.name)
                            .then(() => {
                              setRestoreMsg('Снимок подготовлен — перезапусти приложение.')
                              onSaved()
                            })
                            .catch((e) => setRestoreMsg(`ошибка: ${(e as Error).message}`))
                        }}
                        disabled={!editable}
                        className="ml-auto shrink-0 text-zinc-500 hover:text-acid disabled:opacity-40"
                      >
                        откатить
                      </button>
                      <button
                        onClick={() => {
                          if (confirm('Удалить снимок?')) void api.deleteSnapshot(sn.name).then(loadSnaps)
                        }}
                        className="shrink-0 text-zinc-600 hover:text-magenta"
                      >
                        ×
                      </button>
                    </div>
                  ))}
                </div>
              </div>
            </div>

            <div className="border-t border-line pt-3">
              <div className="mb-1.5 flex items-center gap-2 font-mono text-[10px] uppercase tracking-widest text-zinc-500">
                <GitBranch size={12} className="text-acid" /> обновление из исходников
                {su && (
                  <span className="ml-auto normal-case tracking-normal text-zinc-600">
                    {su.branch} · {su.head}
                    {su.dirty ? ' · локальные правки (уйдут в stash)' : ''}
                  </span>
                )}
              </div>
              {su ? (
                <div className="space-y-2 text-[11px]">
                  {su.behind > 0 ? (
                    <div className="rounded border border-acid/40 bg-acid/5 px-3 py-2">
                      <div className="text-acid">
                        Доступно {su.behind} {su.behind === 1 ? 'новый коммит' : su.behind < 5 ? 'новых коммита' : 'новых коммитов'} в origin/{su.branch}:
                      </div>
                      <ul className="mt-1 space-y-0.5 font-mono text-[10px] text-zinc-300">
                        {su.commits.slice(0, 8).map((c) => (
                          <li key={c} className="truncate">
                            {c}
                          </li>
                        ))}
                      </ul>
                    </div>
                  ) : su.error ? (
                    <div className="rounded border border-magenta/40 bg-magenta/5 px-3 py-2">
                      <div className="text-magenta">Проверить не удалось, поэтому неизвестно, есть ли новое.</div>
                      <div className="mt-1 break-all font-mono text-[10px] text-zinc-500">{su.error}</div>
                      {su.hint && <div className="mt-1 text-zinc-300">{su.hint}</div>}
                    </div>
                  ) : (
                    <div className="text-zinc-500">Ты на последней версии. Проверено {new Date(su.checked_at).toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' })}.</div>
                  )}
                  <div className="flex flex-wrap items-center gap-2">
                    <button
                      onClick={() => {
                        setSuBusy('check')
                        setSuErr(null)
                        void api
                          .updateCheck()
                          .then(setSu)
                          .catch((e) => setSuErr((e as Error).message))
                          .finally(() => setSuBusy('idle'))
                      }}
                      disabled={suBusy !== 'idle'}
                      className="flex items-center gap-1 rounded border border-line px-3 py-1.5 font-mono text-xs text-zinc-300 hover:border-acid hover:text-acid disabled:opacity-40"
                    >
                      <RefreshCw size={12} className={suBusy === 'check' ? 'animate-spin' : ''} /> проверить
                    </button>
                    <button
                      onClick={() => {
                        if (!confirm(`Приложение закроется, в отдельном окне пройдут git pull и сборка (${su.mode === 'desktop' ? 'frontend + tauri, несколько минут' : 'frontend + cargo, пара минут'}), потом оно запустится само. Поехали?`)) return
                        setSuBusy('run')
                        setSuErr(null)
                        void api
                          .updateRun()
                          .then(() => setSuBusy('gone'))
                          .catch((e) => {
                            setSuErr((e as Error).message)
                            setSuBusy('idle')
                          })
                      }}
                      disabled={suBusy !== 'idle' || !editable}
                      className="flex items-center gap-1 rounded bg-acid px-3 py-1.5 font-mono text-xs font-bold text-void hover:shadow-glow disabled:opacity-40"
                      title={su.dirty ? 'Локальные правки (обычно lock-файлы после сборки) будут отложены в git stash и возвращены после обновления' : 'git pull → сборка → перезапуск'}
                    >
                      <ArrowUpCircle size={12} /> {su.behind > 0 ? 'обновить и перезапустить' : 'пересобрать и перезапустить'}
                    </button>
                    {suBusy === 'gone' && <span className="text-acid">закрываюсь… смотри окно сборки, приложение вернётся само</span>}
                  </div>
                  {suErr && <div className="rounded border border-magenta/50 bg-magenta/10 px-3 py-2 text-xs text-magenta">{suErr}</div>}
                  <div className="font-mono text-[10px] text-zinc-600">
                    {su.repo_dir} · {su.mode === 'desktop' ? 'десктоп' : 'cli'} · лог в ~/.triad/update.log
                  </div>
                </div>
              ) : (
                <div className="space-y-2 text-[11px]">
                  {repoDir ? (
                    <div className="text-zinc-500">
                      Клон найден: <code className="text-zinc-300">{repoDir}</code>. Проверяю origin… (при старте это занимает несколько секунд)
                    </div>
                  ) : (
                    <div className="text-zinc-500">
                      Приложение запущено из <code className="text-zinc-300">{exePath}</code>, папку клона рядом не нашёл. Укажи её в поле <code className="text-zinc-300">TRIAD_REPO</code> выше (папка AIPLACE, где лежат <code>.git</code>, <code>frontend</code> и <code>backend</code>), сохрани и нажми «проверить».
                    </div>
                  )}
                  <button
                    onClick={() => {
                      setSuBusy('check')
                      setSuErr(null)
                      void api
                        .updateCheck()
                        .then(setSu)
                        .catch((e) => setSuErr((e as Error).message))
                        .finally(() => setSuBusy('idle'))
                    }}
                    disabled={suBusy !== 'idle'}
                    className="flex items-center gap-1 rounded border border-line px-3 py-1.5 font-mono text-xs text-zinc-300 hover:border-acid hover:text-acid disabled:opacity-40"
                  >
                    <RefreshCw size={12} className={suBusy === 'check' ? 'animate-spin' : ''} /> проверить
                  </button>
                  {suErr && <div className="rounded border border-magenta/50 bg-magenta/10 px-3 py-2 text-xs text-magenta">{suErr}</div>}
                </div>
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
