import { useEffect, useRef, useState } from 'react'
import { ChevronDown, Pencil, Plus, Trash2 } from 'lucide-react'
import type { RoomInfo } from '../types'
import { api, currentRoom, setCurrentRoom } from '../api'

/** Which yard you are in; create, rename, delete yards. Switching reloads the app on the new room. */
export function RoomSwitcher() {
  const [open, setOpen] = useState(false)
  const [rooms, setRooms] = useState<RoomInfo[]>([])
  const [err, setErr] = useState<string | null>(null)
  const box = useRef<HTMLDivElement>(null)
  const cur = currentRoom()
  const load = () => void api.rooms().then(setRooms).catch(() => setRooms([{ id: 'local', name: 'Двор', created_at: '', open: true }]))
  useEffect(() => {
    load()
  }, [])
  useEffect(() => {
    const onDoc = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', onDoc)
    return () => document.removeEventListener('mousedown', onDoc)
  }, [])
  const name = rooms.find((r) => r.id === cur)?.name ?? (cur === 'local' ? 'Двор' : cur)
  const go = (id: string) => {
    setCurrentRoom(id)
    window.location.reload()
  }
  return (
    <div ref={box} className="relative">
      <button onClick={() => setOpen((v) => !v)} className="flex items-center gap-1 rounded border border-line px-2 py-0.5 font-mono text-[11px] text-zinc-200 hover:border-acid" title="Дворы: отдельные комнаты со своими жителями и библиотекой">
        <span className="max-w-[140px] truncate">{name}</span>
        <ChevronDown size={11} className="text-zinc-500" />
      </button>
      {open && (
        <div className="absolute left-0 z-40 mt-1 w-72 rounded border border-line bg-void p-1 shadow-2xl">
          {rooms.map((r) => (
            <div key={r.id} className={`group flex items-center gap-1 rounded px-2 py-1 text-xs ${r.id === cur ? 'bg-zinc-800 text-zinc-100' : 'text-zinc-300 hover:bg-zinc-900'}`}>
              <button onClick={() => r.id !== cur && go(r.id)} className="min-w-0 flex-1 truncate text-left">
                {r.name} {r.id === cur && <span className="font-mono text-[9px] text-acid">· здесь</span>}
              </button>
              <button
                onClick={() => {
                  const n = prompt('Новое название двора:', r.name)
                  if (n && n.trim())
                    void api
                      .renameRoom(r.id, n.trim())
                      .then(setRooms)
                      .catch((e) => setErr((e as Error).message))
                }}
                className="hidden text-zinc-500 hover:text-zinc-100 group-hover:inline"
                title="Переименовать"
              >
                <Pencil size={11} />
              </button>
              {r.id !== 'local' && (
                <button
                  onClick={() => {
                    if (confirm(`Удалить двор «${r.name}»? Файл базы останется рядом как .deleted.`))
                      void api
                        .deleteRoom(r.id)
                        .then((list) => {
                          setRooms(list)
                          if (r.id === cur) go('local')
                        })
                        .catch((e) => setErr((e as Error).message))
                  }}
                  className="hidden text-zinc-500 hover:text-magenta group-hover:inline"
                  title="Удалить"
                >
                  <Trash2 size={11} />
                </button>
              )}
            </div>
          ))}
          <button
            onClick={() => {
              const n = prompt('Название нового двора (например «Работа», «Книги», «Семья»):')
              if (n && n.trim())
                void api
                  .createRoom(n.trim())
                  .then((r) => go(r.id))
                  .catch((e) => setErr((e as Error).message))
            }}
            className="mt-1 flex w-full items-center gap-1 rounded border border-dashed border-acid/50 px-2 py-1 font-mono text-[11px] text-acid hover:bg-acid/10"
          >
            <Plus size={11} /> новый двор
          </button>
          <div className="px-2 pt-1 text-[10px] text-zinc-600">У каждого двора свои жители, библиотека, файлы и бэкапы. Ключ и настройки общие.</div>
          {err && <div className="px-2 pt-1 text-[10px] text-magenta">{err}</div>}
        </div>
      )}
    </div>
  )
}
