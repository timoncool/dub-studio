// PreviewCanvas — the editing heart. Shows the engine's REAL rendered frame (WYSIWYG) with a react-konva
// overlay: drag/resize the blur boxes and drag the subtitle band directly on the frame -> PATCH the Project ->
// re-render the frame. (A 60fps JASSUB live layer over HTML5 <video> is the M2 fast-preview upgrade.)
import { useEffect, useRef, useState } from "react";
import { Stage, Layer, Rect, Line, Transformer } from "react-konva";
import type Konva from "konva";
import { api, type Project } from "../lib/api";
import { useStore } from "../store";

type Props = { pid: string; project: Project; scrub: number; rendered: boolean; onChanged: () => void };

export default function PreviewCanvas({ pid, project, scrub, rendered, onChanged }: Props) {
  const rev = useStore((s) => s.rev);
  const bump = useStore((s) => s.bump);
  const wrap = useRef<HTMLDivElement>(null);
  const trRef = useRef<Konva.Transformer>(null);
  const boxRefs = useRef<Record<number, Konva.Rect>>({});
  const [disp, setDisp] = useState({ w: 0, h: 0 });
  const [sel, setSel] = useState<number | null>(null);
  const [guide, setGuide] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);

  const vw = project.meta.width || 1, vh = project.meta.height || 1;
  const sx = disp.w / vw, sy = disp.h / vh;
  const previewSrc = rendered ? api.outputUrl(pid) : api.previewUrl(pid, scrub, rev);

  // fit the overlay to the displayed media (preserve aspect)
  useEffect(() => {
    const el = wrap.current; if (!el) return;
    const ro = new ResizeObserver(() => {
      const cw = el.clientWidth, ch = el.clientHeight;
      const scale = Math.min(cw / vw, ch / vh);
      setDisp({ w: Math.round(vw * scale), h: Math.round(vh * scale) });
    });
    ro.observe(el); return () => ro.disconnect();
  }, [vw, vh]);

  useEffect(() => {
    if (sel != null && trRef.current && boxRefs.current[sel]) {
      trRef.current.nodes([boxRefs.current[sel]]); trRef.current.getLayer()?.batchDraw();
    } else trRef.current?.nodes([]);
  }, [sel, disp]);

  async function patch(edit: Record<string, unknown>) {
    setBusy(true);
    try { await api.patch(pid, edit); onChanged(); bump(); } finally { setBusy(false); }  // bump -> frame refetches
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
                    onDragEnd={(e) => patch({ op: "subpos", sub_y: Math.round((e.target.y() + 14) / sy) })} />
              {/* blur boxes — drag + resize; drop -> PATCH edit_blur */}
              {blurs.map((b, i) => (
                <Rect key={i} ref={(n) => { if (n) boxRefs.current[i] = n; }}
                      x={b.x * sx} y={b.y * sy} width={b.w * sx} height={b.h * sy}
                      fill={sel === i ? "rgba(91,224,200,0.15)" : "rgba(255,255,255,0.05)"}
                      stroke={sel === i ? "#5be0c8" : "#ffffff66"} strokeWidth={1} draggable
                      onClick={() => setSel(i)} onTap={() => setSel(i)}
                      onDragMove={(e) => setGuide(Math.abs(e.target.x() + (b.w * sx) / 2 - disp.w / 2) < 8 ? disp.w / 2 : null)}
                      onDragEnd={(e) => { setGuide(null); patch({ op: "blur", idx: i, x: Math.round(e.target.x() / sx), y: Math.round(e.target.y() / sy) }); }}
                      onTransformEnd={(e) => {
                        const n = e.target; const w = Math.round((n.width() * n.scaleX()) / sx);
                        const h = Math.round((n.height() * n.scaleY()) / sy);
                        n.scaleX(1); n.scaleY(1);
                        patch({ op: "blur", idx: i, x: Math.round(n.x() / sx), y: Math.round(n.y() / sy), w, h });
                      }} />
              ))}
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
