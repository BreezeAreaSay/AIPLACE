import { useRef, useState } from 'react'
import { Download, FileText, Paperclip, Trash2, X } from 'lucide-react'
import type { Attachment } from '../types'
import { api } from '../api'

interface Props {
  nodeId: string
  attachments: Attachment[]
  onUpload: (files: File[]) => Promise<unknown>
  onDelete: (id: string) => void
  /** ids whose text an inhabitant has already read and digested into the note. */
  digested?: Set<string>
}

export function humanSize(n: number) {
  if (n < 1024) return `${n} Б`
  if (n < 1024 * 1024) return `${Math.round(n / 1024)} КБ`
  return `${(n / 1024 / 1024).toFixed(1)} МБ`
}

export const isImage = (a: Attachment) => a.mime.startsWith('image/')

/** Full-screen viewer for an uploaded image. */
export function Lightbox({ src, name, onClose }: { src: string; name: string; onClose: () => void }) {
  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/90 p-4" onClick={onClose}>
      <img src={src} alt={name} className="max-h-full max-w-full rounded shadow-2xl" />
      <div className="absolute bottom-3 left-0 right-0 text-center font-mono text-[11px] text-zinc-400">{name} · клик — закрыть</div>
      <button onClick={onClose} className="absolute right-3 top-3 rounded bg-black/50 p-1 text-zinc-300 hover:text-white" aria-label="Закрыть">
        <X size={18} />
      </button>
    </div>
  )
}

/** Files of one object: image grid + document list, drag-and-drop or button upload. */
export function Attachments({ nodeId, attachments, onUpload, onDelete, digested }: Props) {
  const [drag, setDrag] = useState(false)
  const [busy, setBusy] = useState(false)
  const [view, setView] = useState<Attachment | null>(null)
  const [textOf, setTextOf] = useState<{ name: string; text: string } | null>(null)
  const input = useRef<HTMLInputElement>(null)
  const mine = attachments.filter((a) => a.node_id === nodeId)
  const images = mine.filter(isImage)
  const docs = mine.filter((a) => !isImage(a))

  const up = async (list: FileList | File[]) => {
    const files = Array.from(list)
    if (!files.length) return
    setBusy(true)
    try {
      await onUpload(files)
    } finally {
      setBusy(false)
    }
  }

  return (
    <section
      className={`rounded ${drag ? 'ring-2 ring-inset ring-acid' : ''}`}
      onDragOver={(e) => {
        if (e.dataTransfer.types.includes('Files')) {
          e.preventDefault()
          e.stopPropagation()
          setDrag(true)
        }
      }}
      onDragLeave={() => setDrag(false)}
      onDrop={(e) => {
        e.preventDefault()
        e.stopPropagation()
        setDrag(false)
        void up(e.dataTransfer.files)
      }}
    >
      <div className="mb-1.5 flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-widest text-zinc-500">
        <Paperclip size={11} /> файлы · {mine.length}
        <button onClick={() => input.current?.click()} disabled={busy} className="ml-auto rounded border border-line px-1.5 py-0.5 text-[10px] normal-case tracking-normal text-zinc-400 hover:border-acid hover:text-acid disabled:opacity-40">
          {busy ? 'гружу…' : '+ файл'}
        </button>
        <input
          ref={input}
          type="file"
          multiple
          className="hidden"
          onChange={(e) => {
            if (e.target.files) void up(e.target.files)
            e.target.value = ''
          }}
        />
      </div>
      {mine.length === 0 && <div className="text-[11px] text-zinc-600">Перетащи сюда картинки, PDF, тексты — документы прочитают и агенты.</div>}
      {images.length > 0 && (
        <div className="mb-2 grid grid-cols-3 gap-1.5">
          {images.map((a) => (
            <div key={a.id} className="group relative aspect-square overflow-hidden rounded border border-line bg-black">
              <img src={api.fileUrl(a.id)} alt={a.name} loading="lazy" className="h-full w-full cursor-zoom-in object-cover" onClick={() => setView(a)} />
              <button
                onClick={() => {
                  if (confirm(`Удалить ${a.name}?`)) onDelete(a.id)
                }}
                className="absolute right-1 top-1 hidden rounded bg-black/70 p-0.5 text-zinc-300 hover:text-magenta group-hover:block"
                title="Удалить"
              >
                <Trash2 size={11} />
              </button>
              <div className="absolute bottom-0 left-0 right-0 truncate bg-black/60 px-1 font-mono text-[9px] text-zinc-300">{a.name}</div>
            </div>
          ))}
        </div>
      )}
      <div className="space-y-1">
        {docs.map((a) => (
          <div key={a.id} className="flex items-center gap-2 rounded border border-line/60 bg-panel/50 px-2 py-1 text-[11px]">
            <FileText size={12} className={a.text_len > 0 ? 'text-acid' : 'text-zinc-500'} />
            <a href={api.fileUrl(a.id)} target="_blank" rel="noreferrer" className="truncate text-zinc-200 hover:underline" title={a.name}>
              {a.name}
            </a>
            <span className="shrink-0 font-mono text-[9px] text-zinc-600">
              {humanSize(a.size)}
              {a.text_len > 0 ? ` · ${a.text_len.toLocaleString('ru-RU')} симв.${digested?.has(a.id) ? ' · прочитан и законспектирован' : ' · в очереди на разбор'}` : ' · без текста'}
            </span>
            {a.text_len > 0 && (
              <button
                onClick={() => void api.fileText(a.id).then((t) => setTextOf({ name: t.name, text: t.text }))}
                className="shrink-0 font-mono text-[9px] text-zinc-500 hover:text-acid"
                title="Показать извлечённый текст"
              >
                текст
              </button>
            )}
            <a href={api.fileUrl(a.id)} download={a.name} className="ml-auto shrink-0 text-zinc-500 hover:text-zinc-200" title="Скачать">
              <Download size={12} />
            </a>
            <button
              onClick={() => {
                if (confirm(`Удалить ${a.name}?`)) onDelete(a.id)
              }}
              className="shrink-0 text-zinc-500 hover:text-magenta"
              title="Удалить"
            >
              <Trash2 size={12} />
            </button>
          </div>
        ))}
      </div>
      {view && <Lightbox src={api.fileUrl(view.id)} name={view.name} onClose={() => setView(null)} />}
      {textOf && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/80 p-4" onClick={() => setTextOf(null)}>
          <div className="max-h-[85vh] w-full max-w-2xl overflow-y-auto rounded border border-line bg-void p-4" onClick={(e) => e.stopPropagation()}>
            <div className="mb-2 flex items-center gap-2 font-mono text-xs text-zinc-300">
              <FileText size={13} className="text-acid" /> {textOf.name}
              <button onClick={() => setTextOf(null)} className="ml-auto text-zinc-500 hover:text-zinc-200" aria-label="Закрыть">
                <X size={14} />
              </button>
            </div>
            <pre className="whitespace-pre-wrap break-words font-mono text-[11px] leading-relaxed text-zinc-300">{textOf.text}</pre>
          </div>
        </div>
      )}
    </section>
  )
}
