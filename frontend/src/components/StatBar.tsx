interface Props {
  label: string
  value: number
  color: string
  max?: number
}

export function StatBar({ label, value, color, max = 100 }: Props) {
  const pct = Math.max(0, Math.min(100, (value / max) * 100))
  return (
    <div className="flex items-center gap-1.5 text-[10px] font-mono">
      <span className="w-6 text-zinc-500 uppercase tracking-wider">{label}</span>
      <div className="relative h-1.5 flex-1 overflow-hidden rounded-sm bg-zinc-800">
        <div
          className="absolute inset-y-0 left-0 transition-all duration-700"
          style={{ width: `${pct}%`, background: color, boxShadow: `0 0 8px ${color}66` }}
        />
      </div>
      <span className="w-6 text-right tabular-nums text-zinc-400">{value}</span>
    </div>
  )
}
