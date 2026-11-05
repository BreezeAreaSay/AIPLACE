import { useEffect, useRef, useState } from 'react'
import { Play } from 'lucide-react'
import { useLive, useNoteScope } from './LiveContext'
import { API_BASE } from '../api'

/**
 * ```js  (or ```js auto)
 * Runs in a sandboxed iframe (opaque origin, no DOM access to the app). The script gets a
 * snapshot of the library and helpers: print(), table(rows), html(str), api.get(path).
 */
const RUNNER = `<!doctype html><meta charset="utf-8"><body style="margin:0;background:#0b0c10;color:#d4d4d8;font:12px/1.5 ui-monospace,monospace;padding:6px">
<div id="out"></div><script>
const out=document.getElementById('out');
const esc=s=>String(s).replace(/[&<>]/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;'}[c]));
function print(...a){const d=document.createElement('div');d.textContent=a.map(x=>typeof x==='object'?JSON.stringify(x,null,1):String(x)).join(' ');out.appendChild(d);post();}
function html(h){const d=document.createElement('div');d.innerHTML=h;out.appendChild(d);post();}
function table(rows){if(!rows||!rows.length){print('(пусто)');return}const cols=Object.keys(rows[0]);let h='<table style="border-collapse:collapse;font-size:11px">';h+='<tr>'+cols.map(c=>'<th style="text-align:left;padding:2px 6px;border-bottom:1px solid #2a2b36;color:#fafafa">'+esc(c)+'</th>').join('')+'</tr>';for(const r of rows){h+='<tr>'+cols.map(c=>'<td style="padding:2px 6px;border-bottom:1px solid #1c1d26">'+esc(r[c]??'')+'</td>').join('')+'</tr>'}h+='</table>';html(h)}
function post(){parent.postMessage({type:'size',h:document.body.scrollHeight+8},'*')}
window.addEventListener('message',async ev=>{const {code,lib,base}=ev.data||{};if(!code)return;out.innerHTML='';
const api={get:async p=>{const r=await fetch(base+p);return r.json()}};
try{const fn=new Function('lib','print','table','html','api','return (async()=>{'+code+'\\n})()');const r=await fn(lib,print,table,html,api);if(r!==undefined)print(r);}catch(e){const d=document.createElement('div');d.style.color='#FF0055';d.textContent='✖ '+(e&&e.message||e);out.appendChild(d)}post();});
parent.postMessage({type:'ready'},'*');
</script></body>`

export function ScriptBlock({ src, lang }: { src: string; lang: string }) {
  const live = useLive()
  const { nodeId } = useNoteScope()
  const auto = /\bauto\b/.test(lang)
  // first line of `src` is the fence info string ("js auto"), the code starts below it
  const code = src.split('\n').slice(1).join('\n')
  const frame = useRef<HTMLIFrameElement>(null)
  const [h, setH] = useState(40)
  const [ran, setRan] = useState(false)
  const [ready, setReady] = useState(false)

  useEffect(() => {
    const onMsg = (e: MessageEvent) => {
      if (e.source !== frame.current?.contentWindow) return
      if (e.data?.type === 'size') setH(Math.min(600, Math.max(32, e.data.h)))
      if (e.data?.type === 'ready') setReady(true)
    }
    window.addEventListener('message', onMsg)
    return () => window.removeEventListener('message', onMsg)
  }, [])

  const run = () => {
    if (!live || !frame.current?.contentWindow) return
    const lib = {
      node: live.nodes.find((n) => n.id === nodeId) ?? null,
      nodes: live.nodes.filter((n) => n.archived === 0),
      edges: live.edges,
      types: live.types,
      attachments: live.attachments,
      agents: live.agents.map((a) => ({ id: a.id, name: a.name, role: a.role, hp: a.hp, dopamine: a.dopamine, active: a.active })),
      now: new Date().toISOString(),
    }
    frame.current.contentWindow.postMessage({ code, lib, base: `${window.location.origin}${API_BASE}` }, '*')
    setRan(true)
  }
  useEffect(() => {
    if (auto && ready && !ran) run()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [auto, ready])

  return (
    <div className="my-2 overflow-hidden rounded border border-line">
      <div className="flex items-center gap-2 border-b border-line bg-panel/60 px-2 py-1 font-mono text-[10px] text-zinc-500">
        <span>js · песочница</span>
        <span className="text-zinc-700">lib.nodes, lib.node, lib.types, print(), table(), html(), api.get()</span>
        <button onClick={run} disabled={!ready} className="ml-auto flex items-center gap-1 rounded border border-acid/50 px-1.5 py-0.5 text-acid hover:bg-acid/10 disabled:opacity-40">
          <Play size={10} /> {ran ? 'ещё раз' : 'выполнить'}
        </button>
      </div>
      <pre className="md-pre !my-0 !rounded-none !border-0 !border-b !border-line max-h-48 overflow-auto">
        <code>{code}</code>
      </pre>
      <iframe ref={frame} title="script" sandbox="allow-scripts" srcDoc={RUNNER} style={{ height: ran ? h : 0, width: '100%', border: 0, display: ran ? 'block' : 'none' }} />
    </div>
  )
}
