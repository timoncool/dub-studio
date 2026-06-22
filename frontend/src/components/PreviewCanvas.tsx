// PreviewCanvas — the editing heart. Shows the engine's REAL rendered frame (WYSIWYG) with a react-konva
// overlay: drag/resize the blur boxes and drag the subtitle band directly on the frame -> PATCH the Project ->
// re-render the frame. (A 60fps JASSUB live layer over HTML5 <video> is the M2 fast-preview upgrade.)
import { useEffect, useRef, useState } from "react";
import { Stage, Layer, Rect, Line, Transformer } from "react-konva";
import type Konva from "konva";
import { api, type Project } from "../lib/api";
import { useStore } from "../store";

type Props = { pid: string; project: Project; scrub: number; rendered: boolean; onChanged: (fresh: Project) => void };

export default function PreviewCanvas({ pid, project, scrub, rendered, onChanged }: Props) {
  const rev = useStore((s) => s.rev);
  const bump = useStore((s) => s.bump);
  const sel = useStore((s) => s.selBlur);        // SHARED with the left blur list (click list <-> click canvas)
  const setSel = useStore((s) => s.setSelBlur);
  const wrap = useRef<HTMLDivElement>(null);
  const trRef = useRef<Konva.Transformer>(null);
  const boxRefs = useRef<Record<number, Konva.Rect>>({});
  const [disp, setDisp] = useState({ w: 0, h: 0 });
  const [guide, setGuide] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);

  const vw = project.meta.width || 1, vh = project.meta.height || 1;
  const sx = disp.w / vw, sy = disp.h / vh;
  const previewSrc = rendered ? api.outputUrl(pid) : api.previewUrl(pid, scrub, rev);

  // fit the overlay to the displayed media (preserve aspect)
  useEffect(() => {
    const el = wrap.current; if (!el) return;
    const measure = () => {
      const cw = el.clientWidth, ch = el.clientHeight;
      if (cw <= 0 || ch <= 0) return;                 // container momentarily 0 -> don't collapse the overlay to 0x0
      const scale = Math.min(cw / vw, ch / vh);
      setDisp({ w: Math.round(vw * scale), h: Math.round(vh * scale) });
    };
    const ro = new ResizeObserver(measure);
    ro.observe(el); measure();                        // measure immediately + on every resize
    return () => ro.disconnect();
  }, [vw, vh]);

  useEffect(() => {
    const node = sel != null ? boxRefs.current[sel] : null;
    const hidden = sel != null && (project.captions.blur_boxes || [])[sel]?.hidden;   // no resize handles on a disabled zone
    if (node && !hidden && trRef.current) {
      trRef.current.nodes([node]); trRef.current.getLayer()?.batchDraw();
    } else trRef.current?.nodes([]);
  }, [sel, disp, project]);

  async function patch(edit: Record<string, unknown>) {
    setBusy(true);
    // api.patch returns the authoritative merged Project -> hand it to the parent. A 2nd GET would clobber an
    // un-persisted tgt_text edit still sitting dirty in the store (textarea not yet blurred).
    try { const fresh = await api.patch(pid, edit); onChanged(fresh); bump(); } finally { setBusy(false); }  // bump -> frame refetches
  }

  const blurs = project.captions.blur_boxes || [];
  const subY = project.captions.sub_y ?? Math.round(vh * 0.82);

  return (
    <div ref={wrap} className="relative w-full h-full min-h-0 overflow-hidden grid place-items-center bg-black/40 rounded-xl">
      <div className="relative" style={{ width: disp.w, height: disp.h }}>
        {rendered
          ? <video src={previewSrc} controls className="absolute inset-0 w-full h-full rounded-lg" />
          : <img src={previewSrc} alt="frame" className="absolute inset-0 w-full h-full rounded-lg" />}
        {!rendered && disp.w > 0 && (
          <Stage width={disp.w} height={disp.h} className="absolute inset-0"
                 onMouseDown={(e) => { if (e.target === e.target.getStage()) setSel(null); }}>
            <Layer>
              {/* subtitle band — draggable vertically; drop -> PATCH sub_y */}
              <Rect x={0} y={subY * sy - 14} width={disp.w} height={28} fill="rgba(198,242,78,0.14)"
                    stroke="#c6f24e" dash={[6, 4]} draggable
                    dragBoundFunc={(p) => ({ x: 0, y: p.y })}
                    onDragEnd={(e) => { if (!sy) return; patch({ op: "subpos", sub_y: Math.round((e.target.y() + 14) / sy) }); }} />
              {/* blur zones — always draggable on the frame (any lane); those active on THIS frame; red = on,
                  cyan = selected, dashed/dim = hidden (blur off). Click selects (synced with the left list). */}
              {blurs.map((b, i) => ({ b, i }))
                .filter(({ b }) => scrub >= b.t0 - 0.6 && scrub <= b.t1 + 0.4)
                .map(({ b, i }) => {
                  const on = sel === i, hid = !!b.hidden;
                  return (
                    <Rect key={i} ref={(n) => { if (n) boxRefs.current[i] = n; }}
                          x={b.x * sx} y={b.y * sy} width={b.w * sx} height={b.h * sy}
                          fill={hid ? "rgba(255,255,255,0.001)" : on ? "rgba(91,224,200,0.18)" : "rgba(255,86,86,0.16)"}
                          stroke={on ? "#5be0c8" : hid ? "#ffffff66" : "#ff5656"} strokeWidth={on ? 2.5 : 2}
                          dash={hid ? [6, 4] : undefined} draggable={!hid}
                          onClick={() => setSel(i)} onTap={() => setSel(i)}
                          onDragMove={(e) => setGuide(Math.abs(e.target.x() + (b.w * sx) / 2 - disp.w / 2) < 8 ? disp.w / 2 : null)}
                          onDragEnd={(e) => { setGuide(null); if (!sx || !sy) return; patch({ op: "blur", idx: i, x: Math.round(e.target.x() / sx), y: Math.round(e.target.y() / sy) }); }}
                          onTransformEnd={(e) => {
                            const n = e.target;
                            if (!sx || !sy) { n.scaleX(1); n.scaleY(1); return; }   // zero-scale -> skip, still reset
                            const w = Math.round((n.width() * n.scaleX()) / sx);
                            const h = Math.round((n.height() * n.scaleY()) / sy);
                            n.scaleX(1); n.scaleY(1);
                            patch({ op: "blur", idx: i, x: Math.round(n.x() / sx), y: Math.round(n.y() / sy), w, h });
                          }} />
                  );
                })}
              {guide != null && <Line points={[guide, 0, guide, disp.h]} stroke="#5be0c8" dash={[4, 4]} />}
              <Transformer ref={trRef} rotateEnabled={false} ignoreStroke
                           boundBoxFunc={(_, b) => ({ ...b, width: Math.max(20, b.width), height: Math.max(12, b.height) })} />
            </Layer>
          </Stage>
        )}
        {busy && <div className="absolute top-2 right-2 text-[11px] text-[var(--color-accent-2)] bg-black/60 px-2 py-0.5 rounded">updating…</div>}
      </div>
    </div>
  );
}
