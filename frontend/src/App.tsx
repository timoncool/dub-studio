import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { motion } from "motion/react";
import { Upload, Languages, AudioLines, Sparkles, ArrowRight, ShieldCheck, Download, Loader2, Trash2, Plus, Captions, Columns2, FolderDown, ExternalLink, X, Undo2, Redo2, Settings, Eye, EyeOff, Play, Pause, RotateCw, RefreshCw, Square, Droplet, Check, HelpCircle, Copy, Star } from "lucide-react";
import { api, type Project, type Capabilities, type ModelStack } from "./lib/api";
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

function SettingsModal({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const [cap, setCap] = useState<Capabilities | null>(null);
  const [m, setM] = useState<ModelStack>({ asr: "", llm: "", vision: "", tts: "" });
  useEffect(() => { api.capabilities().then((c) => { setCap(c); if (c.models) setM(c.models); }).catch(() => {}); }, []);
  const SLOTS: [keyof ModelStack, string][] = [["asr", "ASR"], ["llm", "LLM"], ["vision", "Vision"], ["tts", "TTS"]];
  return (
    <div className="fixed inset-0 z-50 grid place-items-center glass-scrim anim-fade" onClick={onClose}>
      <div className="w-[min(92vw,580px)] rounded-xl glass-panel anim-pop p-5" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between mb-1">
          <span className="font-semibold">{t("settings.title")}</span>
          <button onClick={onClose} className="text-[var(--color-muted)] hover:text-[var(--color-text)]"><X size={16} /></button>
        </div>
        <div className="mono text-[11px] text-[var(--color-muted)] mb-4">{cap ? `${cap.device} · ${cap.tts_quant}` : "…"}</div>
        {SLOTS.map(([k, lbl]) => (
          <div key={k} className="mb-2.5">
            <div className="text-[11px] uppercase tracking-[0.14em] text-[var(--color-muted)] mb-1">{lbl} · {t(`settings.${k}`)}</div>
            <input value={m[k]} onChange={(e) => setM({ ...m, [k]: e.target.value })}
              className="w-full bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-lg px-2.5 py-1.5 text-[12px] mono focus:border-[var(--color-accent)] focus:outline-none" />
          </div>
        ))}
        <button onClick={async () => { await api.setOpts(m).catch(() => {}); onClose(); }}
          className="mt-3 px-4 py-2 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-sm font-semibold hover:brightness-105">{t("common.done")}</button>
      </div>
    </div>
  );
}

const DONATE = {
  boosty: "https://boosty.to/neuro_art",
  dalink: "https://dalink.to/nerual_dreming",
  github: "https://github.com/timoncool/dub-studio",
  telegram: "https://t.me/nerual_dreming",
  crypto: [["BTC", "1E7dHL22RpyhJGVpcvKdbyZgksSYkYeEBC"],
           ["ETH · ERC20", "0xb5db65adf478983186d4897ba92fe2c25c594a0c"],
           ["USDT · TRC20", "TQST9Lp2TjK6FiVkn4fwfGUee7NmkxEE7C"]] as const,
};

function HelpSection({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="mt-4">
      <div className="text-[11px] uppercase tracking-[0.16em] text-[var(--color-muted)] mb-1.5">{title}</div>
      {children}
    </div>
  );
}

function CryptoRow({ coin, addr }: { coin: string; addr: string }) {
  const { t } = useTranslation();
  const [done, setDone] = useState(false);
  return (
    <button title={t("help.copy")}
      onClick={async () => { try { await navigator.clipboard.writeText(addr); setDone(true); setTimeout(() => setDone(false), 1200); } catch { /* clipboard blocked */ } }}
      className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg bg-[var(--color-surface-2)] border border-[var(--color-border)] hover:border-[var(--color-accent)] transition-colors text-left">
      <span className="text-[11px] uppercase tracking-[0.1em] text-[var(--color-muted)] w-[92px] shrink-0">{coin}</span>
      <span className="mono text-[11px] truncate flex-1">{addr}</span>
      {done ? <Check size={13} className="text-[var(--color-accent)] shrink-0" /> : <Copy size={13} className="text-[var(--color-muted)] shrink-0" />}
    </button>
  );
}

function HelpModal({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const how = t("help.how", { returnObjects: true }) as unknown as string[];
  const features = t("help.features", { returnObjects: true }) as unknown as string[];
  const sections = t("help.sections", { returnObjects: true }) as unknown as string[];
  const pay = "flex items-center justify-center gap-2 px-3 py-2 rounded-lg bg-[var(--color-surface-2)] border border-[var(--color-border)] text-[13px] font-medium text-[var(--color-text)] hover:border-[var(--color-accent)] transition-colors";
  const chip = "inline-flex items-center gap-1 px-2 py-1 rounded-md bg-[var(--color-surface-2)] border border-[var(--color-border)] text-[11px] text-[var(--color-muted)] hover:border-[var(--color-accent)] hover:text-[var(--color-text)] transition-colors";
  return (
    <div className="fixed inset-0 z-50 grid place-items-center glass-scrim anim-fade" onClick={onClose}>
      <div className="w-[min(92vw,640px)] max-h-[86vh] overflow-y-auto rounded-xl glass-panel anim-pop p-5" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between mb-2">
          <span className="flex items-center gap-2 font-semibold"><HelpCircle size={17} className="text-[var(--color-accent)]" />{t("help.title")}</span>
          <button onClick={onClose} className="text-[var(--color-muted)] hover:text-[var(--color-text)]"><X size={16} /></button>
        </div>
        <p className="text-[13px] leading-relaxed text-[var(--color-muted)]">{t("help.intro")}</p>

        <HelpSection title={t("help.howTitle")}>
          <ol className="space-y-1.5">
            {how.map((s, i) => (
              <li key={i} className="flex gap-2.5 text-[13px]">
                <span className="grid place-items-center w-5 h-5 shrink-0 rounded-full bg-[var(--color-surface-2)] text-[var(--color-accent)] text-[11px] font-semibold">{i + 1}</span>
                <span className="leading-relaxed">{s}</span>
              </li>
            ))}
          </ol>
        </HelpSection>

        <HelpSection title={t("help.featuresTitle")}>
          <ul className="grid sm:grid-cols-2 gap-x-4 gap-y-1.5">
            {features.map((f, i) => (
              <li key={i} className="flex gap-2 text-[13px] leading-relaxed"><Check size={14} className="text-[var(--color-accent)] shrink-0 mt-[3px]" /><span>{f}</span></li>
            ))}
          </ul>
        </HelpSection>

        <HelpSection title={t("help.sectionsTitle")}>
          <ul className="space-y-1">
            {sections.map((s, i) => <li key={i} className="text-[13px] leading-relaxed text-[var(--color-muted)]">· {s}</li>)}
          </ul>
        </HelpSection>

        <HelpSection title={t("help.donateTitle")}>
          <p className="text-[13px] leading-relaxed text-[var(--color-muted)] mb-3">{t("help.donateIntro")}</p>
          <div className="grid sm:grid-cols-2 gap-2 mb-2.5">
            <a href={DONATE.dalink} target="_blank" rel="noreferrer" className={pay}>💳 {t("help.card")}</a>
            <a href={DONATE.boosty} target="_blank" rel="noreferrer" className={pay}>🚀 {t("help.boostySub")}</a>
          </div>
          <div className="space-y-1.5">
            {DONATE.crypto.map(([c, a]) => <CryptoRow key={c} coin={c} addr={a} />)}
          </div>
          <div className="mt-4 pt-3 border-t border-[var(--color-border)] text-[12px] leading-relaxed text-[var(--color-muted)]">
            {t("help.madeBy")} <a className="text-[var(--color-text)] hover:text-[var(--color-accent)] transition-colors" href={DONATE.telegram} target="_blank" rel="noreferrer">Nerual Dreming</a> — {t("help.founder")} <a className="text-[var(--color-text)] hover:text-[var(--color-accent)] transition-colors" href="https://artgeneration.me" target="_blank" rel="noreferrer">ArtGeneration.me</a>
            <div className="flex flex-wrap gap-1.5 mt-2">
              <a href="https://t.me/neuroport" target="_blank" rel="noreferrer" className={chip}>Нейро-Софт</a>
              <a href={DONATE.github} target="_blank" rel="noreferrer" className={chip}><Star size={11} />GitHub</a>
              <a href={DONATE.telegram} target="_blank" rel="noreferrer" className={chip}>Telegram</a>
            </div>
          </div>
        </HelpSection>
      </div>
    </div>
  );
}

function TopBar() {
  const { t } = useTranslation();
  const [settings, setSettings] = useState(false);
  const [help, setHelp] = useState(false);
  const stage = useStore((s) => s.stage);
  const setStage = useStore((s) => s.setStage);
  const setPid = useStore((s) => s.setPid);
  const setProject = useStore((s) => s.setProject);
  // start over with a new video — the current project stays on disk (reachable via Recent), so no confirm needed
  const newProject = () => {
    setProject(null); setPid(null); setStage("empty");
    try { history.replaceState(null, "", location.pathname); } catch { /* no-op */ }
  };
  return (
    <header className="flex items-center justify-between px-5 h-14 border-b border-[var(--color-border)] bg-[var(--color-surface)]">
      <div className="flex items-center gap-3">
        <span className="flex items-center gap-2 font-bold tracking-tight">
          <img src="/favicon.svg" alt="" width={20} height={20} className="rounded-[5px] shadow-[0_0_10px_rgba(198,242,78,0.25)]" />
          {t("app.name")}
        </span>
        <span className="text-sm text-[var(--color-muted)] hidden sm:inline">{t("app.tagline")}</span>
      </div>
      <div className="flex items-center gap-2">
        {stage !== "empty" && (
          <button onClick={newProject} title={t("nav.newHint")}
            className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-md text-[13px] font-medium text-[var(--color-muted)] hover:text-[var(--color-text)] hover:bg-[var(--color-surface-2)] transition-colors">
            <Plus size={16} /><span className="hidden sm:inline">{t("nav.new")}</span>
          </button>
        )}
        <button onClick={() => setHelp(true)} title={t("help.title")}
          className="p-1.5 rounded-md text-[var(--color-muted)] hover:text-[var(--color-text)] transition-colors"><HelpCircle size={18} /></button>
        <button onClick={() => setSettings(true)} title={t("settings.title")}
          className="p-1.5 rounded-md text-[var(--color-muted)] hover:text-[var(--color-text)] transition-colors"><Settings size={18} /></button>
        <LanguageSwitcher />
      </div>
      {help && <HelpModal onClose={() => setHelp(false)} />}
      {settings && <SettingsModal onClose={() => setSettings(false)} />}
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
    s.setProgress("", "", null);             // fresh stepper for this run
    try {
      const { project_id } = await api.createProject(file);
      s.setPid(project_id);
      const { job_id } = await api.analyze(project_id, tgt, "auto", src);
      await api.watchJob(job_id, (e) => { if (e.type === "progress") s.setProgress(e.stage || "", e.msg || "", e.pct ?? null); });
      s.setProject(await api.getProject(project_id));
      s.setStage("editor");
    } catch (err) {
      s.setProgress("error", String(err), null);  // surface backend failure instead of hanging on "analyzing"
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
          {s.progress.stage === "error" && (   // analyze failed -> stage flips back to "empty"; show WHY here instead of silently returning to the drop screen
            <div className="mt-4 max-w-lg rounded-lg border border-[var(--color-warn)]/40 bg-[color-mix(in_oklab,var(--color-warn)_10%,transparent)] px-3 py-2 text-[13px] text-[var(--color-warn)]">
              <span className="font-semibold">{t("common.error")}</span> · <span className="mono text-[11px] break-words">{s.progress.msg}</span>
            </div>
          )}
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

// editor stages mapped to the engine's stage markers (api._run emits `stage` per _timed block + "download").
const ANALYZE_STEPS: { key: string; stages: string[] }[] = [
  { key: "download",    stages: ["download"] },
  { key: "separating",  stages: ["extract_audio", "separate"] },
  { key: "diarizing",   stages: ["diarize"] },
  { key: "recognizing", stages: ["asr"] },
  { key: "translating", stages: ["translate", "translate_ctx", "rewrite", "rewrite_ctx"] },
  { key: "voicing",     stages: ["tts", "mix"] },        // TTS synthesis + mix — runs BETWEEN translate and OCR; without this the stepper blanks (cur=-1) during voice gen
  { key: "locating",    stages: ["ocr_detect", "translate_titles", "translate_tagline", "build", "burn", "mux"] },
];

function AnalyzeProgress() {
  const { t } = useTranslation();
  const { progress } = useStore();
  const cur = ANALYZE_STEPS.findIndex((stp) => stp.stages.includes(progress.stage));
  const dl = progress.stage === "download";
  const pct = progress.pct;
  return (
    <div className="flex-1 grid place-items-center px-6">
      <div className="w-full max-w-sm">
        <div className="text-center text-lg font-semibold">{t("analyze.title")}</div>
        {dl && <div className="mt-1 text-center text-[12px] text-[var(--color-muted)]">{t("analyze.firstRunNote")}</div>}
        <div className="mt-6 space-y-2.5">
          {ANALYZE_STEPS.map((stp, i) => {
            const done = cur > i, active = cur === i;
            return (
              <div key={stp.key} className="flex items-center gap-3">
                <span className={`grid place-items-center w-5 h-5 shrink-0 rounded-full ${done ? "bg-[var(--color-accent)] text-[var(--color-on-accent)]" : active ? "text-[var(--color-accent)]" : "text-[var(--color-muted)]/40"}`}>
                  {done ? <Check size={12} /> : active ? <Loader2 size={14} className="animate-spin" /> : <span className="w-1.5 h-1.5 rounded-full bg-current" />}
                </span>
                <span className={`text-sm ${active ? "text-[var(--color-text)] font-medium" : done ? "text-[var(--color-muted)]" : "text-[var(--color-muted)]/45"}`}>{t(`analyze.${stp.key}`)}</span>
                {active && dl && pct != null && <span className="ml-auto mono text-[11px] text-[var(--color-accent)]">{Math.round(pct)}%</span>}
              </div>
            );
          })}
        </div>
        <div className="mt-5 h-1.5 w-full rounded-full bg-[var(--color-surface-2)] overflow-hidden">
          {dl && pct != null
            ? <div className="h-full rounded-full bg-[var(--color-accent)] transition-[width] duration-300" style={{ width: `${Math.max(2, Math.min(100, pct))}%` }} />
            : <div className="h-full w-1/3 rounded-full bg-[var(--color-accent)] animate-pulse" />}
        </div>
        <div className="mt-2 min-h-4 text-center mono text-[12px] text-[var(--color-muted)] break-words">{progress.msg}</div>
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

function WaveformTimeline({ pid, duration, scrub, segments, onSeek }: {
  pid: string; duration: number; scrub: number; segments: Project["segments"]; onSeek: (t: number) => void;
}) {
  const [peaks, setPeaks] = useState<number[]>([]);
  const wrap = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(800);
  useEffect(() => { api.waveform(pid).then((r) => setPeaks(r.peaks)).catch(() => {}); }, [pid]);
  useEffect(() => { const el = wrap.current; if (!el) return; const ro = new ResizeObserver(() => setW(el.clientWidth)); ro.observe(el); return () => ro.disconnect(); }, []);
  const h = 40, dur = duration || 1, bw = peaks.length ? w / peaks.length : 1;
  return (
    <div ref={wrap} className="relative w-full overflow-hidden cursor-pointer select-none" style={{ height: h }}
      onClick={(e) => { const r = e.currentTarget.getBoundingClientRect(); onSeek(Math.max(0, Math.min(dur, (e.clientX - r.left) / r.width * dur))); }}>
      <svg width="100%" height={h} viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" className="block">
        {peaks.map((pk, i) => {
          const bh = Math.max(2, pk * (h - 6)), played = (i / peaks.length) * dur <= scrub;
          return <rect key={i} x={(i / peaks.length) * w} y={(h - bh) / 2} width={Math.max(1, bw - 0.5)} height={bh}
                       fill={played ? "var(--color-accent)" : "#3a414c"} opacity={played ? 0.9 : 0.55} />;
        })}
        {segments.map((s, i) => <rect key={"s" + i} x={(s.start / dur) * w} y={0} width={1} height={h} fill="var(--color-muted)" opacity={0.3} />)}
      </svg>
      <div className="absolute top-0 bottom-0 w-px bg-[var(--color-accent)] shadow-[0_0_6px_var(--color-accent)] pointer-events-none" style={{ left: `${(scrub / dur) * 100}%` }} />
    </div>
  );
}

function CommandPalette({ commands }: { commands: { label: string; run: () => void }[] }) {
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") { e.preventDefault(); setOpen((o) => !o); setQ(""); }
      else if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", h); return () => window.removeEventListener("keydown", h);
  }, []);
  if (!open) return null;
  const filtered = commands.filter((c) => c.label.toLowerCase().includes(q.toLowerCase()));
  return (
    <div className="fixed inset-0 z-50 grid place-items-start justify-center pt-[14vh] glass-scrim anim-fade" onClick={() => setOpen(false)}>
      <div className="w-[min(92vw,520px)] rounded-xl glass-panel anim-pop overflow-hidden" onClick={(e) => e.stopPropagation()}>
        <input autoFocus value={q} onChange={(e) => setQ(e.target.value)} placeholder="⌘K  —  команды…"
          className="w-full bg-transparent px-4 py-3 text-[15px] border-b border-[var(--color-border)] focus:outline-none" />
        <div className="max-h-[50vh] overflow-y-auto p-1.5">
          {filtered.map((c, i) => (
            <button key={i} onClick={() => { c.run(); setOpen(false); }}
              className="w-full text-left px-3 py-2 rounded-lg text-[14px] text-[var(--color-text)] hover:bg-[var(--color-surface-2)] transition-colors">{c.label}</button>
          ))}
          {!filtered.length && <div className="px-3 py-4 text-center text-[var(--color-muted)] text-sm">—</div>}
        </div>
      </div>
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
  const bump = useStore((s) => s.bump);
  const selBlur = useStore((s) => s.selBlur);             // selected blur zone (shared with the canvas overlay)
  const setSelBlur = useStore((s) => s.setSelBlur);
  const rendering = useStore((s) => s.rendering);
  const setRendering = useStore((s) => s.setRendering);
  const addExport = useStore((s) => s.addExport);
  const updateExport = useStore((s) => s.updateExport);
  const pushHistory = useStore((s) => s.pushHistory);
  const undo = useStore((s) => s.undo);
  const redo = useStore((s) => s.redo);
  const canUndo = useStore((s) => s.past.length > 0);
  const canRedo = useStore((s) => s.future.length > 0);
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
  const [play, setPlay] = useState(false);                            // dub playback: play TTS audio + advance preview frames + playhead
  const audioRef = useRef<HTMLAudioElement>(null);
  const playEndRef = useRef<number>(Infinity);                        // stop time for single-phrase playback (Infinity = full)
  const [dubRev, setDubRev] = useState(0);                            // dub-audio cache-buster — bumped ONLY when the dub track is re-rendered (regen/export), NOT on every edit, so live edits don't reload <audio> mid-playback
  // Dub playback — drive the EDITOR preview from the dub audio track (no video element): the preview frame and the
  // waveform playhead follow audio.currentTime, so you hear the dub while the future result plays frame-by-frame.
  useEffect(() => {
    const a = audioRef.current; if (!a) return;
    if (!play) { a.pause(); return; }
    setRendered(false); a.play().catch(() => {});
    const id = window.setInterval(() => {
      if (a.currentTime >= playEndRef.current) { a.pause(); setPlay(false); }   // single phrase -> stop at its end
      else setScrub(a.currentTime);
    }, 150);
    return () => window.clearInterval(id);
  }, [play]);   // eslint-disable-line react-hooks/exhaustive-deps
  const [regenId, setRegenId] = useState<string | null>(null);        // segment whose TTS is being re-generated
  const [remixText, setRemixText] = useState("");                     // funny-remix theme/instruction for Gemma
  const [remixing, setRemixing] = useState(false);
  const ss = p.captions.sub_style;
  // one undo snapshot per edit BURST (focus->type->blur), segments AND titles: snapshot on the FIRST change of a
  // field, keyed by field, cleared on blur. Not on focus (that killed redo) nor per-keystroke (that flooded history).
  const burstRef = useRef<string | null>(null);

  function patchSeg(id: string, tgt: string) {                       // instant local echo while typing
    if (burstRef.current !== `seg:${id}`) { pushHistory(p); burstRef.current = `seg:${id}`; }
    setProject({ ...p, segments: p.segments.map((x) => x.id === id ? { ...x, tgt_text: tgt, dirty: true } : x) });
  }
  function titleText(i: number, text: string) {                      // instant local echo for a title's text
    if (burstRef.current !== `title:${i}`) { pushHistory(p); burstRef.current = `title:${i}`; }
    setProject({ ...p, captions: { ...p.captions, titles: p.captions.titles.map((x, j) => j === i ? { ...x, text } : x) } });
  }
  // a patch/PUT rejected (4xx/5xx/offline) -> surface it in the Files panel (like doExport) and re-sync from
  // the server so the optimistic local echo can't silently diverge from persisted truth
  async function surfaceErr(err: unknown) {
    addExport({ id: `err-${Date.now()}`, name: t("common.error"), status: "error", msg: String(err) });
    try { setProject(await api.getProject(pid)); } catch { /* offline -> keep optimistic state */ }
  }
  async function persistSeg(id: string, tgt: string) {               // on blur -> persist to backend + refresh frame
    setRendered(false);
    try { setProject(await api.patch(pid, { op: "segment", id, tgt_text: tgt })); bump(); }
    catch (err) { await surfaceErr(err); }
  }
  async function branch(op: string, extra: Record<string, unknown> = {}) {
    pushHistory(p);                                                  // snapshot for undo BEFORE the mutation
    setRendered(false);
    try { setProject(await api.patch(pid, { op, ...extra })); bump(); }   // style/voice/text change -> re-fetch the frame
    catch (err) { await surfaceErr(err); }
  }
  async function doRegen(segId: string) {                            // re-synthesize the TTS for ONE phrase (mark dirty -> /render)
    if (regenId) return;
    setRegenId(segId);
    try {
      await api.patch(pid, { op: "regen", id: segId });
      const { job_id } = await api.render(pid);                       // re-TTS only the dirty seg + re-mux -> fresh dub
      await api.watchJob(job_id, () => {});
      setProject(await api.getProject(pid)); setRendered(false); bump(); setDubRev(Date.now());   // refresh preview + reload the re-rendered dub audio
    } catch (e) { console.error("regen TTS failed", e); }
    finally { setRegenId(null); }
  }
  async function doRegenAll() {                                      // re-synthesize the WHOLE dub (after switching the pack voice/speaker, or to re-roll)
    if (regenId) return;
    setRegenId("__all__");                                          // sentinel: disables per-seg regen buttons, no per-seg spinner
    try {
      await api.patch(pid, { op: "regen_all" });                    // mark every segment dirty
      const { job_id } = await api.render(pid);                     // re-TTS all dirty segs + re-mux -> fresh dub
      await api.watchJob(job_id, () => {});
      setProject(await api.getProject(pid)); setRendered(false); bump(); setDubRev(Date.now());
    } catch (e) { console.error("regen all TTS failed", e); }
    finally { setRegenId(null); }
  }
  async function forceSeg(seg: Project["segments"][number]) {        // force-refresh ONE phrase: re-seek + re-fetch + re-render (if stuck)
    setScrub(seg.start); setRendered(false);
    setProject(await api.getProject(pid)); bump();
  }
  function playFull() {                                               // bottom-bar Play: play the whole dub from the playhead
    const a = audioRef.current;
    if (play) { setPlay(false); return; }
    playEndRef.current = Infinity; if (a) a.currentTime = scrub; setPlay(true);
  }
  function playSeg(seg: Project["segments"][number]) {               // play JUST this phrase's TTS [start, end]
    const a = audioRef.current; if (!a) return;
    playEndRef.current = seg.end; a.currentTime = seg.start; setScrub(seg.start); setRendered(false);
    if (play) a.play().catch(() => {}); else setPlay(true);
  }
  async function doUndo() { const prev = undo(); if (prev) { setRendered(false); await api.putProject(pid, prev); bump(); } }
  async function doRedo() { const next = redo(); if (next) { setRendered(false); await api.putProject(pid, next); bump(); } }
  useEffect(() => {                                                  // Cmd/Ctrl+Z / Shift+Z / Y (not while typing in a field)
    const h = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      const k = e.key.toLowerCase();
      if (k === "z" && !e.shiftKey) { e.preventDefault(); doUndo(); }
      else if ((k === "z" && e.shiftKey) || k === "y") { e.preventDefault(); doRedo(); }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [pid]);   // eslint-disable-line react-hooks/exhaustive-deps
  async function doExport() {
    const exId = `${pid}-${Date.now()}`;
    const name = (p.meta.video || pid).split(/[\\/]/).pop() || pid;
    addExport({ id: exId, name, status: "rendering", msg: t("common.rendering") });   // queue entry -> Files panel (no screen block)
    setRendering(true);
    try {
      const { job_id } = await api.render(pid);
      await api.watchJob(job_id, (e) => { if (e.type === "progress") updateExport(exId, { msg: e.msg || "" }); });
      updateExport(exId, { status: "done", msg: "", url: `${api.outputUrl(pid)}?rev=${Date.now()}` });   // bust cache on re-export
      setRendered(true); setDubRev(Date.now());   // /dub now serves the freshly rendered output.mp4 -> reload <audio>
    } catch (err) {
      updateExport(exId, { status: "error", msg: String(err) });
    } finally { setRendering(false); }
  }
  async function doRemix() {                                            // Gemma rewrites the WHOLE script on a theme
    if (!remixText.trim() || remixing) return;
    setRemixing(true);
    try {
      pushHistory(p);
      const { job_id } = await api.remix(pid, remixText.trim());
      await api.watchJob(job_id, () => {});
      setRendered(false);
      setProject(await api.getProject(pid));                            // rewritten transcript -> shows in the lane
      bump();
    } catch (err) { console.error("remix failed", err); }
    finally { setRemixing(false); }
  }

  const isActive = (seg: Project["segments"][number]) => scrub >= seg.start && scrub < seg.end;
  const activeId = p.segments.find(isActive)?.id;
  const mode = p.audio.rewrite ? "funny" : (p.mode === "nodub" ? "subtitles" : "dub");   // derived output mode
  const MODES = [["subtitles", Captions], ["dub", AudioLines], ["funny", Sparkles]] as const;
  const cmds = [
    { label: t("mode.subtitles"), run: () => branch("mode", { value: "subtitles" }) },
    { label: t("mode.dub"), run: () => branch("mode", { value: "dub" }) },
    { label: t("mode.funny"), run: () => branch("mode", { value: "funny" }) },
    { label: t("export.proceed"), run: () => doExport() },
    { label: t("common.undo"), run: () => doUndo() },
    { label: t("common.redo"), run: () => doRedo() },
    { label: t("compare.toggle"), run: () => setCompare((c) => !c) },
    ...Object.keys(presets).map((n) => ({ label: `${t("preset.title")}: ${n}`, run: () => branch("preset", { name: n }) })),
  ];
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
                  <span className="ml-auto flex items-center gap-1">
                    {seg.dirty && <span className="text-[var(--color-accent)] text-[10px] mr-0.5" title="edited">●</span>}
                    <button onClick={(e) => { e.stopPropagation(); playSeg(seg); }} title={t("seg.play")}
                      className="text-[var(--color-muted)] hover:text-[var(--color-accent)] transition-colors"><Play size={12} /></button>
                    <button onClick={(e) => { e.stopPropagation(); doRegen(seg.id); }} disabled={regenId !== null} title={t("seg.regen")}
                      className="text-[var(--color-muted)] hover:text-[var(--color-accent)] disabled:opacity-40 transition-colors">
                      {regenId === seg.id ? <Loader2 size={12} className="animate-spin" /> : <RotateCw size={12} />}
                    </button>
                    <button onClick={(e) => { e.stopPropagation(); forceSeg(seg); }} title={t("seg.refreshHint")}
                      className="text-[var(--color-muted)] hover:text-[var(--color-accent)] transition-colors"><RefreshCw size={12} /></button>
                  </span>
                </div>
                <div className="text-[11px] text-[var(--color-muted)]/80 mt-1.5 leading-snug">{seg.src_text}</div>
                <textarea value={seg.tgt_text} onChange={(e) => patchSeg(seg.id, e.target.value)}
                  onClick={(e) => e.stopPropagation()}                       // editing text must not re-seek on every click
                  onBlur={(e) => { burstRef.current = null; persistSeg(seg.id, e.target.value); }}   // end the edit burst
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
                    <div key={i} onClick={() => setSelBlur(i)}
                      className={`flex items-center gap-2 mono text-[10px] rounded px-2 py-1 cursor-pointer transition-colors ${selBlur === i ? "bg-[color-mix(in_oklab,var(--color-accent)_18%,transparent)] text-[var(--color-text)] ring-1 ring-[var(--color-accent)]" : "text-[var(--color-muted)] bg-[var(--color-surface-2)]/40 hover:text-[var(--color-text)]"} ${b.hidden ? "opacity-50" : ""}`}>
                      <button onClick={(e) => { e.stopPropagation(); branch("blur", { idx: i, hidden: !b.hidden }); }}
                        title={b.hidden ? t("blur.show") : t("blur.hide")}
                        className="shrink-0 hover:text-[var(--color-accent)] transition-colors">{b.hidden ? <EyeOff size={12} /> : <Eye size={12} />}</button>
                      <span className="flex-1 truncate">#{i + 1} · {b.w}×{b.h} · {fmtT(b.t0)}{b.hidden ? ` · ${t("blur.off")}` : ""}</span>
                      {b.fill && (
                        <input type="color" value={b.fill} onClick={(e) => e.stopPropagation()}
                          onChange={(e) => { e.stopPropagation(); branch("blur", { idx: i, fill: e.target.value }); }}
                          title={t("blur.fillColor")} className="w-4 h-4 shrink-0 p-0 border-0 bg-transparent rounded cursor-pointer" />
                      )}
                      <button onClick={(e) => { e.stopPropagation(); branch("blur", { idx: i, fill: b.fill ? null : "#000000" }); }}
                        title={b.fill ? t("blur.modeFill") : t("blur.modeBlur")}
                        className="shrink-0 hover:text-[var(--color-accent)] transition-colors">{b.fill ? <Square size={12} /> : <Droplet size={12} />}</button>
                      <button onClick={(e) => { e.stopPropagation(); branch("blur_del", { idx: i }); }} className="shrink-0 hover:text-[var(--color-warn)] transition-colors"><Trash2 size={12} /></button>
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
              <div key={`${ti.start}_${ti.end}_${i}`} className="rounded-xl p-2.5 bg-[var(--color-surface-2)]/50">
                <div className="flex items-center gap-2 mono text-[10px] text-[var(--color-muted)] mb-1.5">
                  <span className="tabnum">{fmtT(ti.start)} → {fmtT(ti.end)}</span>
                  <button onClick={() => branch("title_del", { idx: i })} className="ml-auto hover:text-[var(--color-warn)] transition-colors" title="delete"><Trash2 size={12} /></button>
                </div>
                <input value={ti.text} onChange={(e) => titleText(i, e.target.value)}
                  onBlur={async (e) => { burstRef.current = null; setRendered(false); try { setProject(await api.patch(pid, { op: "title", idx: i, text: e.target.value })); bump(); } catch (err) { await surfaceErr(err); } }}
                  className="w-full bg-[var(--color-bg)]/60 border border-[var(--color-border)] rounded p-1.5 text-[13px] focus:border-[var(--color-accent)] focus:outline-none transition-colors" />
                <div className="flex items-center gap-1.5 mt-1.5 flex-wrap">
                  <button onClick={() => branch("title", { idx: i, bold: !ti.bold })}
                    className={`text-[11px] font-bold px-2 py-0.5 rounded border transition-colors ${ti.bold ? "border-[var(--color-accent)] text-[var(--color-accent)]" : "border-[var(--color-border)] text-[var(--color-muted)]"}`}>{t("style.bold")}</button>
                  <button onClick={() => branch("title", { idx: i, italic: !ti.italic })}
                    className={`text-[11px] italic px-2 py-0.5 rounded border transition-colors ${ti.italic ? "border-[var(--color-accent)] text-[var(--color-accent)]" : "border-[var(--color-border)] text-[var(--color-muted)]"}`}>{t("style.italic")}</button>
                  <button onClick={() => branch("title", { idx: i, uppercase: !ti.uppercase })} title={t("style.caps")}
                    className={`text-[11px] font-semibold px-2 py-0.5 rounded border transition-colors ${ti.uppercase ? "border-[var(--color-accent)] text-[var(--color-accent)]" : "border-[var(--color-border)] text-[var(--color-muted)]"}`}>AA</button>
                  <input type="color" value={ti.color || "#FFFFFF"} onChange={(e) => branch("title", { idx: i, color: e.target.value })}
                    title={t("style.color")} className="w-7 h-6 rounded bg-transparent cursor-pointer border border-[var(--color-border)]" />
                  <input type="color" value={ti.outline || "#000000"} onChange={(e) => branch("title", { idx: i, outline: e.target.value })}
                    title={t("style.outline")} className="w-7 h-6 rounded bg-transparent cursor-pointer border border-dashed border-[var(--color-border)]" />
                  <input key={`ow${i}-${ti.outline_w ?? "a"}`} type="number" min={0} max={20} defaultValue={ti.outline_w ?? undefined} placeholder={t("style.outlineW")} title={t("style.outlineWFull")}
                    onBlur={(e) => branch("title", { idx: i, outline_w: e.target.value === "" ? null : parseInt(e.target.value) })}
                    className="w-12 bg-[var(--color-surface-2)] border border-dashed border-[var(--color-border)] rounded px-1 py-0.5 text-[11px] focus:border-[var(--color-accent)] focus:outline-none" />
                  <select value={ti.shadow_dir ?? ""} title={t("style.shadow")}
                    onChange={(e) => branch("title", { idx: i, shadow_dir: e.target.value === "" ? null : parseInt(e.target.value) })}
                    className="bg-[var(--color-surface-2)] border border-dashed border-[var(--color-border)] rounded px-1 py-0.5 text-[11px] focus:border-[var(--color-accent)] focus:outline-none">
                    <option value="">—</option><option value="270">↑</option><option value="315">↗</option><option value="0">→</option><option value="45">↘</option><option value="90">↓</option><option value="135">↙</option><option value="180">←</option><option value="225">↖</option>
                  </select>
                  <input key={`sz${i}-${ti.size_px ?? "a"}`} type="number" min={12} max={300} defaultValue={ti.size_px ?? undefined} placeholder="px" title={t("style.size")}
                    onBlur={(e) => branch("title", { idx: i, size_px: e.target.value ? parseInt(e.target.value) : null })}
                    className="w-12 bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded px-1 py-0.5 text-[11px] focus:border-[var(--color-accent)] focus:outline-none" />
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

      <main className="flex flex-col min-w-0 min-h-0 overflow-hidden">
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
          <button onClick={doUndo} disabled={!canUndo} title="Ctrl+Z"
            className="p-1.5 rounded-md text-[var(--color-muted)] hover:text-[var(--color-text)] disabled:opacity-30 transition-colors"><Undo2 size={16} /></button>
          <button onClick={doRedo} disabled={!canRedo} title="Ctrl+Shift+Z"
            className="p-1.5 rounded-md text-[var(--color-muted)] hover:text-[var(--color-text)] disabled:opacity-30 transition-colors"><Redo2 size={16} /></button>
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
              onChanged={(fresh) => setProject(fresh)} />
          )}
        </div>
        <div className="flex items-center gap-3 px-4 py-2.5 border-t border-[var(--color-border)] bg-[var(--color-surface)]">
          <button onClick={playFull} title={t("play.dub")}
            className={`shrink-0 p-1.5 rounded-md transition-colors ${play ? "bg-[var(--color-accent)] text-[var(--color-on-accent)]" : "bg-[var(--color-surface-2)] text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
            {play ? <Pause size={15} /> : <Play size={15} />}
          </button>
          <audio ref={audioRef} src={api.dubUrl(pid, dubRev)} onEnded={() => setPlay(false)} preload="auto" className="hidden" />
          <button onClick={() => setCompare((c) => !c)} title={t("compare.toggle")}
            className={`shrink-0 p-1.5 rounded-md transition-colors ${compare ? "bg-[var(--color-accent)] text-[var(--color-on-accent)]" : "bg-[var(--color-surface-2)] text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
            <Columns2 size={15} />
          </button>
          <span className="mono text-[11px] tabnum w-24 shrink-0"><span className="text-[var(--color-accent)] font-semibold">{fmtT(scrub)}</span><span className="text-[var(--color-muted)]"> / {fmtT(p.meta.duration || 0)}</span></span>
          <div className="flex-1 min-w-0">
            <WaveformTimeline pid={pid} duration={p.meta.duration || 0} scrub={scrub} segments={p.segments}
              onSeek={(t) => { setRendered(false); setScrub(t); if (audioRef.current) audioRef.current.currentTime = t; }} />
          </div>
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
            <Row label={t("style.outline")}>
              <div className="flex items-center gap-1.5">
                <input type="color" value={ss.outline || "#000000"} onChange={(e) => branch("caption", { outline: e.target.value })}
                  title={t("style.outline")} className="bg-transparent w-8 h-6 rounded cursor-pointer" />
                <input key={`sow-${ss.outline_w ?? "a"}`} type="number" min={0} max={20} defaultValue={ss.outline_w ?? undefined}
                  placeholder={t("style.outlineW")} title={t("style.outlineWFull")}
                  onBlur={(e) => branch("caption", { outline_w: e.target.value === "" ? null : parseInt(e.target.value) })}
                  className="w-12 bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded px-1 py-0.5 text-[11px] focus:border-[var(--color-accent)] focus:outline-none" />
                <select value={ss.shadow_dir ?? ""} title={t("style.shadow")}
                  onChange={(e) => branch("caption", { shadow_dir: e.target.value === "" ? null : parseInt(e.target.value) })}
                  className="bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded px-1 py-0.5 text-[11px] focus:border-[var(--color-accent)] focus:outline-none">
                  <option value="">—</option><option value="270">↑</option><option value="315">↗</option><option value="0">→</option><option value="45">↘</option><option value="90">↓</option><option value="135">↙</option><option value="180">←</option><option value="225">↖</option>
                </select>
              </div>
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
        {mode === "funny" && (
          <div className="mt-6">
            <SectionLabel>{t("remix.title")}</SectionLabel>
            <textarea value={remixText} onChange={(e) => setRemixText(e.target.value)} rows={2}
              placeholder={t("remix.placeholder")}
              className="w-full bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-lg px-2.5 py-2 text-[13px] resize-none focus:border-[var(--color-accent)] focus:outline-none transition-colors" />
            <button onClick={doRemix} disabled={remixing || !remixText.trim()}
              className="mt-2 w-full inline-flex items-center justify-center gap-2 px-3 py-2 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-sm font-semibold disabled:opacity-50 hover:brightness-105 transition">
              {remixing ? <Loader2 size={15} className="animate-spin" /> : <Sparkles size={15} />}{t("remix.apply")}
            </button>
            <div className="mt-1.5 text-[11px] text-[var(--color-muted)] leading-snug">{t("remix.hint")}</div>
          </div>
        )}
        {mode !== "subtitles" && (
          <>
            <div className="mt-6"><SectionLabel>{t("editor.voice")}</SectionLabel></div>
            <select value={p.audio.voice.mode} onChange={(e) => branch("recast", { voice_mode: e.target.value, voice_name: p.audio.voice.name })}
              className="w-[200px] max-w-full bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-lg px-2.5 py-2 focus:border-[var(--color-accent)] focus:outline-none transition-colors">
              <option value="clone">{t("voice.clone")}</option>
              <option value="autocast">{t("voice.autocast")}</option>
              <option value="voice">{t("voice.pack")}</option>
            </select>
            {p.audio.voice.mode === "voice" && (() => {
              // per-speaker pack voices: engine maps a comma-list to sorted speakers (cycling), so each
              // diarized speaker can get a DISTINCT/funny voice. 1 speaker -> a single picker.
              const spks = [...new Set(p.segments.map((s) => s.speaker ?? "0"))].sort();   // lexical — matches engine sorted()
              const names = (p.audio.voice.name || "").split(",").map((s) => s.trim());
              const pick = (cur: string, on: (v: string) => void) => (
                <select value={cur} onChange={(e) => on(e.target.value)}
                  className="w-[200px] max-w-full bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-lg px-2.5 py-1.5 text-[13px] focus:border-[var(--color-accent)] focus:outline-none transition-colors">
                  <option value="">{voiceList.length ? "—" : "(пак не найден)"}</option>
                  {voiceList.map((v) => <option key={v} value={v}>{v}</option>)}
                </select>
              );
              if (spks.length <= 1)
                return <div className="mt-2 flex">{pick(names[0] || "", (v) => branch("recast", { voice_mode: "voice", voice_name: v }))}</div>;
              return (
                <div className="mt-2 space-y-1.5">
                  {spks.map((spk, i) => (
                    <div key={spk} className="flex items-center gap-2">
                      <span className="mono text-[10px] text-[var(--color-muted)] w-12 shrink-0">SPK {spk}</span>
                      {pick(names[i] || "", (v) => branch("recast", {
                        voice_mode: "voice",
                        voice_name: spks.map((_, j) => (j === i ? v : names[j] || "")).join(","),
                      }))}
                    </div>
                  ))}
                </div>
              );
            })()}
            <button onClick={doRegenAll} disabled={regenId !== null} title={t("voice.regenAll")}
              className="mt-3 w-full inline-flex items-center justify-center gap-2 px-3 py-2 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-sm font-semibold disabled:opacity-50 hover:brightness-105 transition">
              {regenId === "__all__" ? <Loader2 size={15} className="animate-spin" /> : <RotateCw size={15} />}{t("voice.regenAll")}
            </button>
          </>
        )}

      </aside>
      <CommandPalette commands={cmds} />
    </div>
  );
}

// Non-blocking export queue: a floating Files panel (bottom-right). You keep editing while renders run; each
// finished file gets Download + Open. Subscribes only to `exports`, so it repaints independently of the editor.
function FilesPanel() {
  const { t } = useTranslation();
  const exports = useStore((s) => s.exports);
  const [open, setOpen] = useState(true);
  if (!exports.length) return null;
  const active = exports.filter((e) => e.status === "rendering").length;
  return (
    <div className="fixed bottom-4 right-4 z-40 w-[min(90vw,340px)] flex flex-col items-end">
      {open && (
        <div className="mb-2 w-full rounded-xl glass-panel anim-pop overflow-hidden">
          <div className="flex items-center justify-between px-3 py-2 border-b border-[var(--color-border)]">
            <span className="mono text-[11px] uppercase tracking-[0.14em] text-[var(--color-muted)]">{t("files.title")}</span>
            <button onClick={() => setOpen(false)} className="text-[var(--color-muted)] hover:text-[var(--color-text)]"><X size={14} /></button>
          </div>
          <div className="max-h-[52vh] overflow-y-auto p-2 space-y-1.5">
            {exports.map((e) => (
              <div key={e.id} className="rounded-lg bg-[var(--color-surface-2)]/60 p-2.5">
                <div className="flex items-center gap-2">
                  {e.status === "rendering" ? <Loader2 size={14} className="animate-spin text-[var(--color-accent)] shrink-0" />
                    : e.status === "error" ? <span className="text-[var(--color-warn)] shrink-0 font-bold">!</span>
                    : <span className="w-1.5 h-1.5 rounded-full bg-[var(--color-accent)] shrink-0" />}
                  <span className="text-[13px] truncate flex-1">{e.name}</span>
                </div>
                {e.status === "rendering" && (
                  <>
                    <div className="mt-1.5 h-1 w-full rounded-full bg-[var(--color-surface-2)] overflow-hidden"><div className="h-full w-1/3 rounded-full bg-[var(--color-accent)] anim-indeterminate" /></div>
                    <div className="mt-1 mono text-[10px] text-[var(--color-muted)] truncate">{e.msg}</div>
                  </>
                )}
                {e.status === "done" && e.url && (
                  <div className="mt-2 flex gap-1.5">
                    <a href={`${e.url}&dl=1`} className="flex-1 inline-flex items-center justify-center gap-1.5 text-[12px] py-1 rounded-md bg-[var(--color-accent)] text-[var(--color-on-accent)] font-semibold"><Download size={13} /> {t("files.download")}</a>
                    <a href={e.url} target="_blank" rel="noreferrer" className="inline-flex items-center justify-center gap-1.5 text-[12px] px-2.5 py-1 rounded-md border border-[var(--color-border)] text-[var(--color-muted)] hover:text-[var(--color-text)]"><ExternalLink size={13} /> {t("files.open")}</a>
                  </div>
                )}
                {e.status === "error" && <div className="mt-1 mono text-[10px] text-[var(--color-warn)] truncate">{e.msg}</div>}
              </div>
            ))}
          </div>
        </div>
      )}
      <button onClick={() => setOpen((o) => !o)}
        className="flex items-center gap-2 px-3.5 py-2 rounded-full bg-[var(--color-overlay)] border border-[var(--color-border)] shadow-lg text-[13px] hover:border-[#3a414c] transition-colors">
        {active ? <Loader2 size={15} className="animate-spin text-[var(--color-accent)]" /> : <FolderDown size={15} className="text-[var(--color-accent)]" />}
        {t("files.title")} <span className="mono text-[11px] text-[var(--color-muted)]">{exports.length}</span>
      </button>
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
      <FilesPanel />
    </div>
  );
}
