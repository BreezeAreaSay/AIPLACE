import { useEffect, useRef, useState } from 'react'
import mermaid from 'mermaid'

mermaid.initialize({
  startOnLoad: false,
  theme: 'base',
  securityLevel: 'loose',
  fontFamily: 'JetBrains Mono, ui-monospace, monospace',
  themeVariables: {
    darkMode: true,
    background: '#0D0E12',
    primaryColor: '#13141a',
    primaryTextColor: '#f4f4f5',
    primaryBorderColor: '#3a3b47',
    lineColor: '#5b5d6e',
    secondaryColor: '#13141a',
    tertiaryColor: '#0D0E12',
    fontSize: '13px',
  },
  flowchart: { htmlLabels: true, curve: 'basis', padding: 12, nodeSpacing: 28, rankSpacing: 36 },
})

let counter = 0

interface Props {
  code: string
  className?: string
  /** `fit` scales the diagram to the container; `natural` keeps 1:1 size and lets the host scroll. */
  mode?: 'fit' | 'natural'
}

function naturalWidth(svg: string): number | undefined {
  const m = svg.match(/viewBox="[\d.\-]+ [\d.\-]+ ([\d.]+) [\d.]+"/)
  return m ? Math.ceil(parseFloat(m[1])) : undefined
}

export function MermaidView({ code, className, mode = 'fit' }: Props) {
  const [svg, setSvg] = useState<string>('')
  const [error, setError] = useState<string | null>(null)
  const host = useRef<HTMLDivElement>(null)

  useEffect(() => {
    let cancelled = false
    const id = `triad-${++counter}`
    ;(async () => {
      try {
        const ok = await mermaid.parse(code, { suppressErrors: true })
        if (!ok) throw new Error('Mermaid не смог распарсить схему')
        const { svg } = await mermaid.render(id, code)
        if (!cancelled) {
          setSvg(svg)
          setError(null)
        }
      } catch (e) {
        if (!cancelled) setError((e as Error).message ?? String(e))
        // mermaid leaves a dangling element on failure
        document.getElementById('d' + id)?.remove()
      }
    })()
    return () => {
      cancelled = true
    }
  }, [code])

  if (error) {
    return (
      <div className={`space-y-2 ${className ?? ''}`}>
        <div className="rounded border border-magenta/50 bg-magenta/5 px-3 py-2 text-xs font-mono text-magenta">
          Схема не отрисовалась: {error}
        </div>
        <pre className="overflow-auto rounded border border-line bg-black/40 p-3 text-[11px] text-zinc-400">{code}</pre>
      </div>
    )
  }
  const width = mode === 'natural' ? naturalWidth(svg) : undefined
  return (
    <div className={`overflow-x-auto ${className ?? ''}`}>
      <div
        ref={host}
        className={`mermaid-host ${mode === 'natural' ? 'mermaid-natural' : ''}`}
        style={width ? { width, minWidth: width } : undefined}
        dangerouslySetInnerHTML={{ __html: svg }}
      />
    </div>
  )
}
