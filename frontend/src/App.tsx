import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { motion } from "motion/react";
import { Upload, Languages, AudioLines, Sparkles, ArrowRight, ShieldCheck, Download, Loader2, Trash2, Plus, Captions, Columns2 } from "lucide-react";
import { api, type Project } from "./lib/api";
import { LANGS, setLang, type Lang } from "./lib/i18n";
import { useStore } from "./store";
import PreviewCanvas from "./components/PreviewCanvas";

const EASE = [0.22, 1, 0.36, 1] as const;

// timecode m:ss.d — the navigation reference shown on every segment + the scrub readout
function fmtT(s: number) {
  const m = Math.floor(s / 60), sec = Math.floor(s % 60), d = Math.floor((s * 10) % 10);
  return `${m}:${String(sec).padStart(2, "0")}.${d}`;
}

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
  const { t, i18n } = useTranslation();
  const s = useStore();
  const inputRef = useRef<HTMLInputElement>(null);
  const [over, setOver] = useState(false);
  const [tgt, setTgt] = useState<string>((i18n.language as string) || "ru");   // translate TO (default = UI lang)
  const [src, setSrc] = useState("auto");                                       // translate FROM (auto-detect)

  async function start(file: File) {
    s.setStage("analyzing");
    try {
      const { project_id } = await api.createProject(file);
      s.setPid(project_id);
      const { job_id } = await api.analyze(project_id, tgt, "auto", src);
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
          <div className="mt-3.5 flex items-center justify-center gap-2 text-[12px]">
            <Languages size={14} className="text-[var(--color-accent-2)]" />
            <select value={src} onChange={(e) => setSrc(e.target.value)}
              className="bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded px-1.5 py-0.5 text-[11px] text-[var(--color-text)] focus:border-[var(--color-accent)] focus:outline-none">
              <option value="auto">auto</option>
              {LANGS.map((l) => <option key={l} value={l}>{l.toUpperCase()}</option>)}
            </select>
            <ArrowRight size={12} className="text-[var(--color-muted)]" />
            <select value={tgt} onChange={(e) => setTgt(e.target.value)}
              className="bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded px-1.5 py-0.5 text-[11px] text-[var(--color-text)] focus:border-[var(--color-accent)] focus:outline-none">
              {LANGS.map((l) => <option key={l} value={l}>{l.toUpperCase()}</option>)}
            </select>
          </div>
          <div className="mt-2 flex items-center justify-center gap-2 text-[12px] text-[var(--color-muted)]">
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

function ComparePane({ label, src }: { label: string; src: string }) {
  return (
    <div className="relative h-full min-h-0 grid place-items-center bg-black/40 rounded-xl overflow-hidden">
      <img src={src} alt={label} className="max-h-full max-w-full object-contain" />
      <span className="absolute top-2 left-2 mono text-[10px] px-2 py-0.5 rounded bg-black/60 text-[var(--color-text)] border border-[var(--color-border)]">{label}</span>
    </div>
  );
}

function Editor() {
  const { t } = useTranslation();
  const p = useStore((s) => s.project) as Project;
  const rev = useStore((s) => s.rev);   // for the compare 'result' pane refetch (PreviewCanvas reads rev itself)        // subscribe ONLY to what we render -> no re-render on
  const pid = useStore((s) => s.pid) as string;           // rev bumps or progress SSE ticks (those go to PreviewCanvas)
  const rendered = useStore((s) => s.rendered);
  const setProject = useStore((s) => s.setProject);
  const setRendered = useStore((s) => s.setRendered);
  const setProgress = useStore((s) => s.setProgress);
  const bump = useStore((s) => s.bump);
  const rendering = useStore((s) => s.rendering);
  const setRendering = useStore((s) => s.setRendering);
  const [scrub, setScrub] = useState(1.0);
  const [fonts, setFonts] = useState<Record<string, string>>({});
  const [voiceList, setVoiceList] = useState<string[]>([]);
  const [presets, setPresets] = useState<Record<string, Record<string, unknown>>>({});
  useEffect(() => { api.fonts().then((r) => setFonts(r.fonts)).catch(() => {}); }, []);   // bundled caption fonts
  useEffect(() => { api.voices().then((r) => setVoiceList(r.voices)).catch(() => {}); }, []);   // pack voices
  useEffect(() => { api.presets().then((r) => setPresets(r.presets)).catch(() => {}); }, []);   // caption look presets
  const [sizeDraft, setSizeDraft] = useState<number | null>(null);   // live size while dragging (commit on release)
  const [lane, setLane] = useState<"subs" | "blur" | "titles">("subs"); // left lane: which object type to edit
  const [blurAll, setBlurAll] = useState(false);                      // blur: only active-on-frame vs all zones
  const [compare, setCompare] = useState(false);                      // before/after split preview (Topaz-style)
  const ss = p.captions.sub_style;

  function patchSeg(id: string, tgt: string) {                       // instant local echo while typing
    setProject({ ...p, segments: p.segments.map((x) => x.id === id ? { ...x, tgt_text: tgt, dirty: true } : x) });
  }
  function titleText(i: number, text: string) {                      // instant local echo for a title's text
    setProject({ ...p, captions: { ...p.captions, titles: p.captions.titles.map((x, j) => j === i ? { ...x, text } : x) } });
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
        <div className="inline-flex rounded-lg bg-[var(--color-surface-2)] p-0.5 border border-[var(--color-border)] mb-3 text-[12px]">
          {([["subs", t("mode.subtitles")], ["blur", `${t("blur.title")} ${(p.captions.blur_boxes || []).length}`],
            ["titles", `${t("titles.tab")} ${(p.captions.titles || []).length}`]] as const).map(([k, lbl]) => (
            <button key={k} onClick={() => setLane(k as typeof lane)}
              className={`px-2.5 py-1 rounded-md transition-colors ${lane === k ? "bg-[var(--color-accent)] text-[var(--color-on-accent)] font-semibold" : "text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
              {lbl}
            </button>
          ))}
        </div>
        {lane === "subs" && (
        <div className="space-y-2">
          {p.segments.map((seg) => {
            const on = isActive(seg);
            return (
              <div key={seg.id} ref={on ? activeRef : undefined}
                onClick={() => { setRendered(false); setScrub(seg.start); }}   // click a phrase -> seek the playhead to it
                className={`rounded-xl p-2.5 border-l-2 transition-colors cursor-pointer ${on ? "bg-[var(--color-surface-2)] border-[var(--color-accent)]" : "bg-[var(--color-surface-2)]/40 border-transparent hover:bg-[var(--color-surface-2)]/70"}`}>
                <div className="flex items-center gap-2">
                  <span className={`mono text-[11px] px-1.5 py-0.5 rounded tabnum ${on ? "bg-[var(--color-accent)] text-[var(--color-on-accent)] font-semibold" : "bg-[var(--color-overlay)] text-[var(--color-muted)]"}`}>{fmtT(seg.start)}</span>
                  <span className="mono text-[10px] text-[var(--color-muted)]/60 tabnum">→ {fmtT(seg.end)}</span>
                  {seg.speaker != null && <span className="mono px-1.5 py-px rounded bg-[var(--color-overlay)] text-[9px] text-[var(--color-muted)]">SPK {seg.speaker}</span>}
                  {seg.dirty && <span className="ml-auto text-[var(--color-accent)] text-[10px]" title="edited">●</span>}
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
        )}
        {lane === "blur" && (
          <div className="space-y-2">
            <Toggle label={t("blur.on")} on={p.render.blur} onClick={() => branch("blur_enable", { on: !p.render.blur })} />
            <div className={p.render.blur ? "" : "opacity-40 pointer-events-none"}>
              <div className="flex items-center justify-between mt-2 mb-1.5">
                <span className="mono text-[10px] text-[var(--color-muted)]">{blurAll ? `${t("blur.all")} · ${(p.captions.blur_boxes || []).length}` : t("blur.frame")}</span>
                <button onClick={() => setBlurAll(!blurAll)} className="mono text-[10px] text-[var(--color-accent)] hover:underline">
                  {blurAll ? t("blur.frame") : `${t("blur.all")} (${(p.captions.blur_boxes || []).length})`}
                </button>
              </div>
              <div className="space-y-1 max-h-[46vh] overflow-y-auto pr-1">
                {(p.captions.blur_boxes || []).map((b, i) => ({ b, i }))
                  .filter(({ b }) => blurAll || (scrub >= b.t0 - 0.6 && scrub <= b.t1 + 0.4))
                  .map(({ b, i }) => (
                    <div key={i} className="flex items-center justify-between mono text-[10px] text-[var(--color-muted)] bg-[var(--color-surface-2)]/40 rounded px-2 py-1">
                      <span>#{i + 1} · {b.w}×{b.h} · {fmtT(b.t0)}</span>
                      <button onClick={() => branch("blur_del", { idx: i })} className="hover:text-[var(--color-warn)] transition-colors"><Trash2 size={12} /></button>
                    </div>
                  ))}
                {!(p.captions.blur_boxes || []).some((b) => blurAll || (scrub >= b.t0 - 0.6 && scrub <= b.t1 + 0.4)) &&
                  <div className="text-[11px] text-[var(--color-muted)]/50 py-3 text-center">—</div>}
              </div>
              <button onClick={() => branch("blur_add", { x: Math.round((p.meta.width || 0) * 0.25), y: Math.round((p.meta.height || 0) * 0.45), w: Math.round((p.meta.width || 0) * 0.5), h: Math.round((p.meta.height || 0) * 0.08), t0: Math.max(0, scrub - 1), t1: scrub + 2 })}
                className="w-full mt-1.5 inline-flex items-center justify-center gap-1.5 text-[12px] py-1.5 rounded-lg border border-dashed border-[var(--color-border)] text-[var(--color-muted)] hover:border-[var(--color-accent)] hover:text-[var(--color-text)] transition-colors">
                <Plus size={13} /> {t("blur.add")}
              </button>
            </div>
          </div>
        )}
        {lane === "titles" && (
          <div className="space-y-2">
            {!(p.captions.titles || []).length && <div className="text-[11px] text-[var(--color-muted)]/50 py-3 text-center">—</div>}
            {(p.captions.titles || []).map((ti, i) => (
              <div key={i} className="rounded-xl p-2.5 bg-[var(--color-surface-2)]/50">
                <div className="flex items-center gap-2 mono text-[10px] text-[var(--color-muted)] mb-1.5">
                  <span className="tabnum">{fmtT(ti.start)} → {fmtT(ti.end)}</span>
                  <button onClick={() => branch("title_del", { idx: i })} className="ml-auto hover:text-[var(--color-warn)] transition-colors" title="delete"><Trash2 size={12} /></button>
                </div>
                <input value={ti.text} onChange={(e) => titleText(i, e.target.value)} onBlur={(e) => branch("title", { idx: i, text: e.target.value })}
                  className="w-full bg-[var(--color-bg)]/60 border border-[var(--color-border)] rounded p-1.5 text-[13px] focus:border-[var(--color-accent)] focus:outline-none transition-colors" />
                <div className="flex items-center gap-1.5 mt-1.5 flex-wrap">
                  <button onClick={() => branch("title", { idx: i, bold: !ti.bold })}
                    className={`text-[11px] font-bold px-2 py-0.5 rounded border transition-colors ${ti.bold ? "border-[var(--color-accent)] text-[var(--color-accent)]" : "border-[var(--color-border)] text-[var(--color-muted)]"}`}>{t("style.bold")}</button>
                  <button onClick={() => branch("title", { idx: i, italic: !ti.italic })}
                    className={`text-[11px] italic px-2 py-0.5 rounded border transition-colors ${ti.italic ? "border-[var(--color-accent)] text-[var(--color-accent)]" : "border-[var(--color-border)] text-[var(--color-muted)]"}`}>{t("style.italic")}</button>
                  <input type="color" value={ti.color || "#FFFFFF"} onChange={(e) => branch("title", { idx: i, color: e.target.value })}
                    title={t("style.color")} className="w-7 h-6 rounded bg-transparent cursor-pointer border border-[var(--color-border)]" />
                  <select value={ti.font || ""} onChange={(e) => branch("title", { idx: i, font: e.target.value })}
                    className="bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded px-1.5 py-0.5 text-[11px] focus:border-[var(--color-accent)] focus:outline-none">
                    <option value="">{t("style.font")}</option>
                    {Object.keys(fonts).map((f) => <option key={f} value={f}>{f}</option>)}
                  </select>
                </div>
              </div>
            ))}
            <button onClick={() => branch("title_add", { text: "Title", x: Math.round((p.meta.width || 0) * 0.15), y: Math.round((p.meta.height || 0) * 0.4), w: Math.round((p.meta.width || 0) * 0.7), h: Math.round((p.meta.height || 0) * 0.1), t0: Math.max(0, scrub - 0.5), t1: scrub + 3 })}
              className="w-full inline-flex items-center justify-center gap-1.5 text-[12px] py-1.5 rounded-lg border border-dashed border-[var(--color-border)] text-[var(--color-muted)] hover:border-[var(--color-accent)] hover:text-[var(--color-text)] transition-colors">
              <Plus size={13} /> {t("titles.add")}
            </button>
          </div>
        )}
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
          {compare ? (
            <div className="w-full h-full grid grid-cols-2 gap-2 min-h-0">
              <ComparePane label={t("compare.original")} src={api.originalUrl(pid, scrub)} />
              <ComparePane label={t("compare.result")} src={api.previewUrl(pid, scrub, rev)} />
            </div>
          ) : (
            <PreviewCanvas pid={pid} project={p} scrub={scrub} rendered={rendered}
              onChanged={async () => setProject(await api.getProject(pid))} />
          )}
        </div>
        <div className="flex items-center gap-3 px-4 py-2.5 border-t border-[var(--color-border)] bg-[var(--color-surface)]">
          <button onClick={() => setCompare((c) => !c)} title={t("compare.toggle")}
            className={`shrink-0 p-1.5 rounded-md transition-colors ${compare ? "bg-[var(--color-accent)] text-[var(--color-on-accent)]" : "bg-[var(--color-surface-2)] text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
            <Columns2 size={15} />
          </button>
          <span className="mono text-[11px] tabnum w-24 shrink-0"><span className="text-[var(--color-accent)] font-semibold">{fmtT(scrub)}</span><span className="text-[var(--color-muted)]"> / {fmtT(p.meta.duration || 0)}</span></span>
          <input type="range" min={0} max={p.meta.duration || 1} step={0.1} value={scrub}
            onChange={(e) => { setRendered(false); setScrub(parseFloat(e.target.value)); }} className="w-full accent-[var(--color-accent)]" />
        </div>
      </main>

      <aside className="border-l border-[var(--color-border)] overflow-y-auto p-4 bg-[var(--color-surface)] text-sm">
        <SectionLabel>{t("preset.title")}</SectionLabel>
        <div className="grid grid-cols-2 gap-1.5 mb-5 max-h-52 overflow-y-auto pr-1">
          <button onClick={() => branch("preset", { name: "" })}
            className={`text-[11px] px-2 py-1.5 rounded-lg border transition-colors ${!p.captions.preset?.name ? "border-[var(--color-accent)] text-[var(--color-accent)] bg-[color-mix(in_oklab,var(--color-accent)_8%,transparent)]" : "border-[var(--color-border)] text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
            {t("preset.original")}
          </button>
          {Object.keys(presets).map((name) => (
            <button key={name} onClick={() => branch("preset", { name })}
              className={`text-[11px] px-2 py-1.5 rounded-lg border truncate transition-colors ${p.captions.preset?.name === name ? "border-[var(--color-accent)] text-[var(--color-accent)] bg-[color-mix(in_oklab,var(--color-accent)_8%,transparent)]" : "border-[var(--color-border)] text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
              {name}
            </button>
          ))}
        </div>
        <SectionLabel>{t("editor.style")}</SectionLabel>
        {ss && (
          <div className="space-y-3">
            <Row label={t("style.font")}>
              <select value={ss.font || "Montserrat"} onChange={(e) => branch("caption", { font: e.target.value })}
                className="bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-md px-2 py-1 text-[12px] max-w-[160px] focus:border-[var(--color-accent)] focus:outline-none transition-colors"
                title={fonts[ss.font || "Montserrat"] || ""}>
                {Object.keys(fonts).length ? Object.keys(fonts).map((f) => <option key={f} value={f}>{f}</option>)
                                           : <option value={ss.font || "Montserrat"}>{ss.font || "Montserrat"}</option>}
              </select>
            </Row>
            <Toggle label={t("style.bold")} on={ss.bold} onClick={() => branch("caption", { bold: !ss.bold })} />
            <Toggle label={t("style.italic")} on={ss.italic} onClick={() => branch("caption", { italic: !ss.italic })} />
            <Toggle label={t("style.caps")} on={ss.uppercase} onClick={() => branch("caption", { uppercase: !ss.uppercase })} />
            <Row label={t("style.color")}>
              <input type="color" value={ss.color} onChange={(e) => branch("caption", { color: e.target.value })}
                className="bg-transparent w-8 h-6 rounded cursor-pointer" />
            </Row>
            <div>
              <div className="flex items-center justify-between">
                <span className="text-[var(--color-muted)]">{t("style.size")}</span>
                <span className="mono text-[11px] text-[var(--color-text)]">{sizeDraft ?? ss.size_px ?? Math.round((p.meta.height || 1280) / 14)}px</span>
              </div>
              <input type="range" min={24} max={Math.round((p.meta.height || 1280) / 5)}
                value={sizeDraft ?? ss.size_px ?? Math.round((p.meta.height || 1280) / 14)}
                onChange={(e) => setSizeDraft(parseInt(e.target.value))}
                onPointerUp={async () => { if (sizeDraft != null) { await branch("caption", { size_px: sizeDraft }); setSizeDraft(null); } }}
                className="w-full mt-1.5 accent-[var(--color-accent)]" />
            </div>
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
            {p.audio.voice.mode === "voice" && (
              <select value={p.audio.voice.name || ""} onChange={(e) => branch("recast", { voice_mode: "voice", voice_name: e.target.value })}
                className="w-full mt-2 bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-lg px-2.5 py-2 text-[13px] focus:border-[var(--color-accent)] focus:outline-none transition-colors">
                <option value="">{voiceList.length ? "—" : "(пак не найден)"}</option>
                {voiceList.map((v) => <option key={v} value={v}>{v}</option>)}
              </select>
            )}
          </>
        )}

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
