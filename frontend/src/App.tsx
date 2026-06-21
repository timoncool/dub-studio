import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { motion } from "motion/react";
import { Upload, Languages, AudioLines, Sparkles, ArrowRight, ShieldCheck, Download, Loader2, Trash2, Plus, Captions } from "lucide-react";
import { api, type Project } from "./lib/api";
import { LANGS, setLang, type Lang } from "./lib/i18n";
import { useStore } from "./store";
import PreviewCanvas from "./components/PreviewCanvas";

const EASE = [0.22, 1, 0.36, 1] as const;

function LanguageSwitcher() {
  const { i18n } = useTranslation();
  return (
    <select
      value={i18n.language as Lang}
      onChange={(e) => setLang(e.target.value as Lang)}
      className="bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-md px-2 py-1 text-sm"
    >
      {LANGS.map((l) => <option key={l} value={l}>{l.toUpperCase()}</option>)}
    </select>
  );
}

function TopBar() {
  const { t } = useTranslation();
  return (
    <header className="flex items-center justify-between px-5 h-14 border-b border-[var(--color-border)] bg-[var(--color-surface)]">
      <div className="flex items-center gap-3">
        <span className="flex items-center gap-2 font-bold tracking-tight">
          <span className="w-2.5 h-2.5 rounded-[3px] bg-[var(--color-accent)] shadow-[0_0_10px_var(--color-accent)]" />
          {t("app.name")}
        </span>
        <span className="text-sm text-[var(--color-muted)] hidden sm:inline">{t("app.tagline")}</span>
      </div>
      <LanguageSwitcher />
    </header>
  );
}

function Feature({ icon: Icon, title, desc, delay }: { icon: typeof Languages; title: string; desc: string; delay: number }) {
  return (
    <motion.div initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.45, ease: EASE, delay }}
      className="flex items-start gap-3.5">
      <div className="mt-0.5 grid place-items-center w-9 h-9 shrink-0 rounded-lg bg-[var(--color-surface-2)] border border-[var(--color-border)] text-[var(--color-accent)]">
        <Icon size={18} strokeWidth={2} />
      </div>
      <div>
        <div className="font-semibold text-[15px] leading-tight">{title}</div>
        <div className="text-sm text-[var(--color-muted)] mt-0.5">{desc}</div>
      </div>
    </motion.div>
  );
}

function DropZone() {
  const { t } = useTranslation();
  const s = useStore();
  const inputRef = useRef<HTMLInputElement>(null);
  const [over, setOver] = useState(false);

  async function start(file: File) {
    s.setStage("analyzing");
    try {
      const { project_id } = await api.createProject(file);
      s.setPid(project_id);
      const { job_id } = await api.analyze(project_id, "ru", "auto");
      await api.watchJob(job_id, (e) => { if (e.type === "progress") s.setProgress(e.stage || "", e.msg || ""); });
      s.setProject(await api.getProject(project_id));
      s.setStage("editor");
    } catch (err) {
      s.setProgress("error", String(err));  // surface backend failure instead of hanging on "analyzing"
      s.setStage("empty");
    }
  }

  const corner = `absolute w-4 h-4 rounded-[2px] transition-colors duration-200 ${over ? "border-[var(--color-accent)]" : "border-[var(--color-border)]"}`;
  return (
    <div className="flex-1 min-h-0 overflow-y-auto grid place-items-center px-6 py-10">
      <motion.div initial={{ opacity: 0, y: 14 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.5, ease: EASE }}
        className="w-full max-w-5xl grid lg:grid-cols-[1.05fr_0.95fr] gap-12 items-center">

        <div>
          <div className="mono text-[11px] tracking-[0.2em] text-[var(--color-muted)] flex items-center gap-2">
            <span className="w-1.5 h-1.5 rounded-full bg-[var(--color-accent)] shadow-[0_0_8px_var(--color-accent)]" />{t("hero.eyebrow")}
          </div>
          <h1 className="mt-5 text-3xl lg:text-[36px] leading-[1.06] font-extrabold tracking-tight max-w-xl">{t("hero.headline")}</h1>
          <p className="mt-4 text-[15px] leading-relaxed text-[var(--color-muted)] max-w-lg">{t("hero.sub")}</p>
          <div className="mt-8 space-y-4">
            <Feature icon={Languages} title={t("actions.translate")} desc={t("hero.f1")} delay={0.12} />
            <Feature icon={AudioLines} title={t("actions.dub")} desc={t("hero.f2")} delay={0.18} />
            <Feature icon={Sparkles} title={t("actions.funny")} desc={t("hero.f3")} delay={0.24} />
          </div>
        </div>

        <motion.div initial={{ opacity: 0, scale: 0.98 }} animate={{ opacity: 1, scale: 1 }} transition={{ duration: 0.5, ease: EASE, delay: 0.1 }}>
          <div
            onDragOver={(e) => { e.preventDefault(); setOver(true); }}
            onDragLeave={() => setOver(false)}
            onDrop={(e) => { e.preventDefault(); setOver(false); const f = e.dataTransfer.files?.[0]; if (f) start(f); }}
            onClick={() => inputRef.current?.click()}
            className={`group relative aspect-[4/3] rounded-2xl border grid place-items-center cursor-pointer overflow-hidden transition-all duration-200
              ${over ? "border-[var(--color-accent)] bg-[color-mix(in_oklab,var(--color-accent)_9%,var(--color-surface))] shadow-[0_0_0_4px_color-mix(in_oklab,var(--color-accent)_18%,transparent)]"
                     : "border-[var(--color-border)] bg-[var(--color-surface)] hover:bg-[var(--color-surface-2)] hover:border-[#3a414c]"}`}
          >
            <span className={`${corner} top-3 left-3 border-l border-t`} />
            <span className={`${corner} top-3 right-3 border-r border-t`} />
            <span className={`${corner} bottom-3 left-3 border-l border-b`} />
            <span className={`${corner} bottom-3 right-3 border-r border-b`} />
            <div className="text-center px-6">
              <div className={`mx-auto grid place-items-center w-16 h-16 rounded-2xl border transition-all duration-200
                ${over ? "bg-[var(--color-accent)] text-[var(--color-on-accent)] border-transparent" : "bg-[var(--color-surface-2)] text-[var(--color-accent)] border-[var(--color-border)] group-hover:scale-105"}`}>
                <Upload size={26} strokeWidth={2} />
              </div>
              <div className="mt-5 text-lg font-semibold">{t("drop.title")}</div>
              <div className="mt-1.5 text-sm text-[var(--color-muted)]">{t("drop.hint")}</div>
              <span className="mt-5 inline-flex items-center gap-2 px-4 py-2 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-sm font-semibold">
                {t("drop.browse")} <ArrowRight size={16} />
              </span>
            </div>
          </div>
          <div className="mt-3.5 flex items-center justify-center gap-2 text-[12px] text-[var(--color-muted)]">
            <ShieldCheck size={14} className="text-[var(--color-accent-2)]" /> {t("hero.formats")}
          </div>
        </motion.div>
      </motion.div>
      <input ref={inputRef} type="file" accept="video/mp4,video/quicktime" className="hidden"
        onChange={(e) => { const f = e.target.files?.[0]; if (f) start(f); }} />
    </div>
  );
}

function AnalyzeProgress() {
  const { t } = useTranslation();
  const { progress } = useStore();
  return (
    <div className="flex-1 grid place-items-center">
      <div className="text-center">
        <div className="text-lg font-medium">{t("analyze.title")}</div>
        <div className="mt-3 h-1.5 w-64 mx-auto rounded-full bg-[var(--color-surface-2)] overflow-hidden">
          <div className="h-full w-1/3 bg-[var(--color-accent)] animate-pulse" />
        </div>
        <div className="mt-2 text-sm text-[var(--color-muted)] min-h-5">{progress.msg}</div>
      </div>
    </div>
  );
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex items-center gap-2 text-[11px] uppercase tracking-[0.16em] text-[var(--color-muted)] mb-3">
      <span className="w-1 h-1 rounded-full bg-[var(--color-accent)]" />{children}
    </div>
  );
}
function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return <div className="flex items-center justify-between"><span className="text-[var(--color-muted)]">{label}</span><span>{children}</span></div>;
}
function Toggle({ label, on, onClick }: { label: string; on: boolean; onClick: () => void }) {
  return (
    <button onClick={onClick} className="flex items-center justify-between w-full">
      <span className="text-[var(--color-muted)]">{label}</span>
      <span className={`w-9 h-5 rounded-full p-0.5 transition-colors ${on ? "bg-[var(--color-accent)]" : "bg-[var(--color-surface-2)]"}`}>
        <span className={`block w-4 h-4 rounded-full bg-white transition-transform ${on ? "translate-x-4" : ""}`} />
      </span>
    </button>
  );
}

function Editor() {
  const { t } = useTranslation();
  const p = useStore((s) => s.project) as Project;        // subscribe ONLY to what we render -> no re-render on
  const pid = useStore((s) => s.pid) as string;           // rev bumps or progress SSE ticks (those go to PreviewCanvas)
  const rendered = useStore((s) => s.rendered);
  const setProject = useStore((s) => s.setProject);
  const setRendered = useStore((s) => s.setRendered);
  const setProgress = useStore((s) => s.setProgress);
  const bump = useStore((s) => s.bump);
  const rendering = useStore((s) => s.rendering);
  const setRendering = useStore((s) => s.setRendering);
  const [scrub, setScrub] = useState(1.0);
  const ss = p.captions.sub_style;

  function patchSeg(id: string, tgt: string) {                       // instant local echo while typing
    setProject({ ...p, segments: p.segments.map((x) => x.id === id ? { ...x, tgt_text: tgt, dirty: true } : x) });
  }
  async function persistSeg(id: string, tgt: string) {               // on blur -> persist to backend + refresh frame
    setRendered(false);
    setProject(await api.patch(pid, { op: "segment", id, tgt_text: tgt }));
    bump();
  }
  async function branch(op: string, extra: Record<string, unknown> = {}) {
    setRendered(false);
    setProject(await api.patch(pid, { op, ...extra }));
    bump();                                                          // style/voice/text change -> re-fetch the frame
  }
  async function doExport() {
    setRendering(true);
    try {
      const { job_id } = await api.render(pid);
      setProgress("render", t("common.rendering"));
      await api.watchJob(job_id, (e) => { if (e.type === "progress") setProgress(e.stage || "", e.msg || ""); });
      setRendered(true);
    } finally { setRendering(false); }
  }

  const isActive = (seg: Project["segments"][number]) => scrub >= seg.start && scrub < seg.end;
  const activeId = p.segments.find(isActive)?.id;
  const mode = p.audio.rewrite ? "funny" : (p.mode === "nodub" ? "subtitles" : "dub");   // derived output mode
  const MODES = [["subtitles", Captions], ["dub", AudioLines], ["funny", Sparkles]] as const;
  const activeRef = useRef<HTMLDivElement>(null);
  useEffect(() => { activeRef.current?.scrollIntoView({ block: "nearest", behavior: "smooth" }); }, [activeId]);
  return (
    <div className="flex-1 grid grid-cols-[330px_1fr_300px] min-h-0">
      <aside className="border-r border-[var(--color-border)] overflow-y-auto p-4 bg-[var(--color-surface)]">
        <SectionLabel>{t("editor.transcript")}</SectionLabel>
        <div className="space-y-2">
          {p.segments.map((seg) => {
            const on = isActive(seg);
            return (
              <div key={seg.id} ref={on ? activeRef : undefined}
                onClick={() => { setRendered(false); setScrub(seg.start); }}   // click a phrase -> seek the playhead to it
                className={`rounded-xl p-2.5 border-l-2 transition-colors cursor-pointer ${on ? "bg-[var(--color-surface-2)] border-[var(--color-accent)]" : "bg-[var(--color-surface-2)]/40 border-transparent hover:bg-[var(--color-surface-2)]/70"}`}>
                <div className="flex items-center gap-2 mono text-[10px] text-[var(--color-muted)]">
                  <span>{seg.start.toFixed(1)}–{seg.end.toFixed(1)}s</span>
                  {seg.speaker != null && <span className="px-1.5 py-px rounded bg-[var(--color-overlay)] text-[9px]">SPK {seg.speaker}</span>}
                  {seg.dirty && <span className="ml-auto text-[var(--color-accent)]" title="edited">● ред.</span>}
                </div>
                <div className="text-[11px] text-[var(--color-muted)]/80 mt-1.5 leading-snug">{seg.src_text}</div>
                <textarea value={seg.tgt_text} onChange={(e) => patchSeg(seg.id, e.target.value)}
                  onClick={(e) => e.stopPropagation()}                       // editing text must not re-seek on every click
                  onBlur={(e) => persistSeg(seg.id, e.target.value)}
                  className="w-full mt-1.5 bg-[var(--color-bg)]/60 border border-[var(--color-border)] rounded-lg p-1.5 text-[13px] leading-snug resize-none focus:border-[var(--color-accent)] focus:outline-none transition-colors" rows={2} />
              </div>
            );
          })}
        </div>
      </aside>

      <main className="flex flex-col min-w-0 min-h-0">
        <div className="flex items-center gap-3 px-4 py-2.5 border-b border-[var(--color-border)] bg-[var(--color-surface)]">
          <div className="inline-flex rounded-lg bg-[var(--color-surface-2)] p-0.5 border border-[var(--color-border)] shrink-0">
            {MODES.map(([k, Ic]) => (
              <button key={k} onClick={() => branch("mode", { value: k })}
                className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[13px] transition-colors ${mode === k ? "bg-[var(--color-accent)] text-[var(--color-on-accent)] font-semibold" : "text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
                <Ic size={15} /> {t(`mode.${k}`)}
              </button>
            ))}
          </div>
          <span className="text-[12px] text-[var(--color-muted)] truncate hidden xl:inline">{t(`mode.${mode}_desc`)}</span>
          <div className="flex-1" />
          <button onClick={doExport} disabled={rendering}
            className="inline-flex items-center gap-2 px-4 py-1.5 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-sm font-semibold disabled:opacity-70 hover:brightness-105 transition shrink-0">
            {rendering ? <Loader2 size={16} className="animate-spin" /> : <Download size={16} />}{t("export.proceed")}
          </button>
        </div>
        <div className="flex-1 min-h-0 p-3 overflow-hidden">
          <PreviewCanvas pid={pid} project={p} scrub={scrub} rendered={rendered}
            onChanged={async () => setProject(await api.getProject(pid))} />
        </div>
        <div className="flex items-center gap-3 px-4 py-2.5 border-t border-[var(--color-border)] bg-[var(--color-surface)]">
          <span className="mono text-[10px] text-[var(--color-muted)] tabnum w-20 shrink-0">{scrub.toFixed(1)} / {(p.meta.duration || 0).toFixed(1)}s</span>
          <input type="range" min={0} max={p.meta.duration || 1} step={0.1} value={scrub}
            onChange={(e) => { setRendered(false); setScrub(parseFloat(e.target.value)); }} className="w-full accent-[var(--color-accent)]" />
        </div>
      </main>

      <aside className="border-l border-[var(--color-border)] overflow-y-auto p-4 bg-[var(--color-surface)] text-sm">
        <SectionLabel>{t("editor.style")}</SectionLabel>
        {ss && (
          <div className="space-y-3">
            <Row label={t("style.font")}><span className="mono text-[12px] text-[var(--color-text)]">{ss.font || "—"}</span></Row>
            <Toggle label={t("style.bold")} on={ss.bold} onClick={() => branch("caption", { bold: !ss.bold })} />
            <Toggle label={t("style.italic")} on={ss.italic} onClick={() => branch("caption", { italic: !ss.italic })} />
            <Toggle label={t("style.caps")} on={ss.uppercase} onClick={() => branch("caption", { uppercase: !ss.uppercase })} />
            <Row label={t("style.color")}>
              <input type="color" value={ss.color} onChange={(e) => branch("caption", { color: e.target.value })}
                className="bg-transparent w-8 h-6 rounded cursor-pointer" />
            </Row>
          </div>
        )}
        {mode !== "subtitles" && (
          <>
            <div className="mt-6"><SectionLabel>{t("editor.voice")}</SectionLabel></div>
            <select value={p.audio.voice.mode} onChange={(e) => branch("recast", { voice_mode: e.target.value })}
              className="w-full bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-lg px-2.5 py-2 focus:border-[var(--color-accent)] focus:outline-none transition-colors">
              <option value="clone">{t("voice.clone")}</option>
              <option value="autocast">{t("voice.autocast")}</option>
              <option value="voice">{t("voice.pack")}</option>
            </select>
          </>
        )}

        <div className="mt-6 flex items-center justify-between">
          <SectionLabel>{t("blur.title")}</SectionLabel>
          <span className="mono text-[10px] text-[var(--color-muted)]">{(p.captions.blur_boxes || []).length} зон</span>
        </div>
        <Toggle label={t("blur.on")} on={p.render.blur} onClick={() => branch("blur_enable", { on: !p.render.blur })} />
        <div className={`mt-2.5 space-y-1 ${p.render.blur ? "" : "opacity-40 pointer-events-none"}`}>
          <div className="max-h-40 overflow-y-auto space-y-1 pr-1">
            {(p.captions.blur_boxes || []).map((b, i) => (
              <div key={i} className="flex items-center justify-between mono text-[10px] text-[var(--color-muted)] bg-[var(--color-surface-2)]/40 rounded px-2 py-0.5">
                <span>#{i + 1} · {b.w}×{b.h}</span>
                <button onClick={() => branch("blur_del", { idx: i })} className="hover:text-[var(--color-warn)] transition-colors" title="delete"><Trash2 size={12} /></button>
              </div>
            ))}
          </div>
          <button onClick={() => branch("blur_add", { x: Math.round((p.meta.width || 0) * 0.25), y: Math.round((p.meta.height || 0) * 0.45), w: Math.round((p.meta.width || 0) * 0.5), h: Math.round((p.meta.height || 0) * 0.08) })}
            className="w-full inline-flex items-center justify-center gap-1.5 text-[12px] py-1.5 rounded-lg border border-dashed border-[var(--color-border)] text-[var(--color-muted)] hover:border-[var(--color-accent)] hover:text-[var(--color-text)] transition-colors">
            <Plus size={13} /> {t("blur.add")}
          </button>
        </div>
      </aside>
    </div>
  );
}

function ExportOverlay() {
  const { t } = useTranslation();
  const rendering = useStore((s) => s.rendering);   // dedicated subscriber: progress ticks repaint ONLY this overlay
  const msg = useStore((s) => s.progress.msg);
  if (!rendering) return null;
  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/55 backdrop-blur-sm">
      <div className="w-[min(92vw,440px)] rounded-2xl border border-[var(--color-border)] bg-[var(--color-overlay)]/95 p-6 shadow-[0_20px_60px_rgba(0,0,0,0.5)]">
        <div className="flex items-center gap-2.5">
          <Loader2 size={18} className="animate-spin text-[var(--color-accent)]" />
          <span className="font-semibold">{t("export.title")}</span>
        </div>
        <div className="mt-4 h-1.5 w-full rounded-full bg-[var(--color-surface-2)] overflow-hidden">
          <div className="h-full w-1/3 rounded-full bg-[var(--color-accent)] anim-indeterminate" />
        </div>
        <div className="mt-3 mono text-[11px] text-[var(--color-muted)] min-h-4 truncate">{msg || t("common.rendering")}</div>
      </div>
    </div>
  );
}

export default function App() {
  const stage = useStore((s) => s.stage);                 // only re-route on stage change (not on every store write)
  const setPid = useStore((s) => s.setPid);
  const setProject = useStore((s) => s.setProject);
  const setStage = useStore((s) => s.setStage);
  const [cap, setCap] = useState("");
  useEffect(() => { api.capabilities().then((c) => setCap(`${c.device} · ${c.asr_model}`)).catch(() => setCap("backend offline")); }, []);
  // open an existing project directly via ?pid=... (resume / dev)
  useEffect(() => {
    const pid = new URLSearchParams(location.search).get("pid");
    if (pid) api.getProject(pid).then((p) => { setPid(pid); setProject(p); setStage("editor"); }).catch(() => {});
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
  return (
    <div className="h-full flex flex-col">
      <TopBar />
      {stage === "empty" && <DropZone />}
      {stage === "analyzing" && <AnalyzeProgress />}
      {stage === "editor" && <Editor />}
      <footer className="mono h-6 px-4 flex items-center gap-2 text-[10px] text-[var(--color-muted)] border-t border-[var(--color-border)] bg-[var(--color-surface)]">
        <span className="w-1.5 h-1.5 rounded-full bg-[var(--color-accent)]" />{cap}
      </footer>
      <ExportOverlay />
    </div>
  );
}
