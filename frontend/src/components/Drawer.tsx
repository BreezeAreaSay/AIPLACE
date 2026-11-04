import type { ReactNode } from 'react'
import { X } from 'lucide-react'

interface Props {
  open: boolean
  onClose: () => void
  title: string
  children: ReactNode
}

/** Bottom sheet for the TMA / narrow layout. */
export function Drawer({ open, onClose, title, children }: Props) {
  if (!open) return null
  return (
    <div className="fixed inset-0 z-40 flex flex-col justify-end md:hidden">
      <div className="absolute inset-0 bg-black/70" onClick={onClose} />
      <div className="relative flex h-[82vh] flex-col rounded-t-xl border-t border-line bg-void shadow-2xl">
        <div className="flex items-center justify-between border-b border-line px-3 py-2">
          <span className="font-mono text-xs uppercase tracking-widest text-zinc-400">{title}</span>
          <button onClick={onClose} className="rounded p-1 text-zinc-500 hover:text-zinc-200" aria-label="Закрыть">
            <X size={16} />
          </button>
        </div>
        <div className="min-h-0 flex-1 overflow-hidden">{children}</div>
      </div>
    </div>
  )
}
