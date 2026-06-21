import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type Project } from "./lib/api";
import { LANGS, setLang, type Lang } from "./lib/i18n";
import { useStore } from "./store";
import PreviewCanvas from "./components/PreviewCanvas";

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
      <div className="flex items-baseline gap-3">
        <span className="font-semibold tracking-tight">{t("app.name")}</span>
        <span className="text-sm text-[var(--color-muted)] hidden sm:inline">{t("app.tagline")}</span>
      </div>
      <LanguageSwitcher />
    </header>
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

  return (
    <div className="flex-1 grid place-items-center p-8">
      <div
        onDragOver={(e) => { e.preventDefault(); setOver(true); }}
        onDragLeave={() => setOver(false)}
        onDrop={(e) => { e.preventDefault(); setOver(false); const f = e.dataTransfer.files?.[0]; if (f) start(f); }}
        onClick={() => inputRef.current?.click()}
        className={`w-full max-w-2xl aspect-video rounded-2xl border-2 border-dashed grid place-items-center cursor-pointer transition-colors ${over ? "border-[var(--color-accent)] bg-[color-mix(in_oklab,var(--color-accent)_8%,transparent)]" : "border-[var(--color-border)] bg-[var(--color-surface)]"}`}
      >
        <div className="text-center">
          <div className="text-xl font-medium">{t("drop.title")}</div>
          <div className="text-sm text-[var(--color-muted)] mt-1">{t("drop.hint")}</div>
          <button className="mt-4 px-4 py-2 rounded-lg bg-[var(--color-accent)] text-white text-sm font-medium">{t("drop.browse")}</button>
        </div>
      </div>
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
  const s = useStore();
  const p = s.project as Project;
  const pid = s.pid as string;
  const [scrub, setScrub] = useState(1.0);
  const ss = p.captions.sub_style;

  function patchSeg(id: string, tgt: string) {
    s.setProject({ ...p, segments: p.segments.map((x) => x.id === id ? { ...x, tgt_text: tgt, dirty: true } : x) });
  }
  async function branch(op: string, extra: Record<string, unknown> = {}) {
    s.setRendered(false);
    s.setProject(await api.patch(pid, { op, ...extra }));
  }
  async function doExport() {
    const { job_id } = await api.render(pid);
    s.setProgress("render", t("common.rendering"));
    await api.watchJob(job_id, (e) => { if (e.type === "progress") s.setProgress(e.stage || "", e.msg || ""); });
    s.setRendered(true);
  }

  return (
    <div className="flex-1 grid grid-cols-[320px_1fr_300px] min-h-0">
      <aside className="border-r border-[var(--color-border)] overflow-y-auto p-3 bg-[var(--color-surface)]">
        <div className="text-xs uppercase tracking-wide text-[var(--color-muted)] mb-2">{t("editor.transcript")}</div>
        {p.segments.map((seg) => (
          <div key={seg.id} className="mb-2 rounded-lg bg-[var(--color-surface-2)] p-2">
            <div className="text-[11px] text-[var(--color-muted)]">{seg.start.toFixed(1)}–{seg.end.toFixed(1)}s{seg.dirty ? " ·●" : ""}</div>
            <div className="text-[12px] text-[var(--color-muted)] mt-1">{seg.src_text}</div>
            <textarea value={seg.tgt_text} onChange={(e) => patchSeg(seg.id, e.target.value)}
              className="w-full mt-1 bg-transparent border border-[var(--color-border)] rounded p-1 text-sm resize-none" rows={2} />
          </div>
        ))}
      </aside>

      <main className="flex flex-col min-w-0">
        <div className="flex gap-2 px-4 py-2 border-b border-[var(--color-border)] bg-[var(--color-surface)]">
          <button onClick={() => branch("translate", { lang: p.tgt_lang })} className="px-3 py-1.5 rounded-md bg-[var(--color-surface-2)] text-sm">{t("actions.translate")}</button>
          <button onClick={() => branch("recast", { voice_mode: "clone" })} className="px-3 py-1.5 rounded-md bg-[var(--color-surface-2)] text-sm">{t("actions.dub")}</button>
          <button onClick={() => branch("rewrite", { instruction: "make it a funny, playful dub" })} className="px-3 py-1.5 rounded-md bg-[var(--color-surface-2)] text-sm">{t("actions.funny")}</button>
          <div className="flex-1" />
          <button onClick={doExport} className="px-4 py-1.5 rounded-md bg-[var(--color-accent)] text-white text-sm font-medium">{t("export.proceed")}</button>
        </div>
        <div className="flex-1 min-h-0 p-2">
          <PreviewCanvas pid={pid} project={p} scrub={scrub} rendered={s.rendered}
            onChanged={async () => s.setProject(await api.getProject(pid))} />
        </div>
        <div className="px-4 py-2 border-t border-[var(--color-border)] bg-[var(--color-surface)]">
          <input type="range" min={0} max={p.meta.duration || 1} step={0.1} value={scrub}
            onChange={(e) => { s.setRendered(false); setScrub(parseFloat(e.target.value)); }} className="w-full" />
        </div>
      </main>

      <aside className="border-l border-[var(--color-border)] overflow-y-auto p-3 bg-[var(--color-surface)] text-sm">
        <div className="text-xs uppercase tracking-wide text-[var(--color-muted)] mb-2">{t("editor.style")}</div>
        {ss && (
          <div className="space-y-2">
            <Row label={t("style.font")}>{ss.font || "—"}</Row>
            <Toggle label={t("style.bold")} on={ss.bold} onClick={() => branch("caption", { bold: !ss.bold })} />
            <Toggle label={t("style.italic")} on={ss.italic} onClick={() => branch("caption", { italic: !ss.italic })} />
            <Toggle label={t("style.caps")} on={ss.uppercase} onClick={() => branch("caption", { uppercase: !ss.uppercase })} />
            <Row label={t("style.color")}>
              <input type="color" value={ss.color} onChange={(e) => branch("caption", { color: e.target.value })} className="bg-transparent w-8 h-6" />
            </Row>
          </div>
        )}
        <div className="text-xs uppercase tracking-wide text-[var(--color-muted)] mt-4 mb-2">{t("editor.voice")}</div>
        <select value={p.audio.voice.mode} onChange={(e) => branch("recast", { voice_mode: e.target.value })}
          className="w-full bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-md px-2 py-1">
          <option value="clone">{t("voice.clone")}</option>
          <option value="autocast">{t("voice.autocast")}</option>
          <option value="voice">{t("voice.pack")}</option>
        </select>
      </aside>
    </div>
  );
}

export default function App() {
  const stage = useStore((s) => s.stage);
  const [cap, setCap] = useState("");
  useEffect(() => { api.capabilities().then((c) => setCap(`${c.device} · ${c.asr_model}`)).catch(() => setCap("backend offline")); }, []);
  return (
    <div className="h-full flex flex-col">
      <TopBar />
      {stage === "empty" && <DropZone />}
      {stage === "analyzing" && <AnalyzeProgress />}
      {stage === "editor" && <Editor />}
      <footer className="h-6 px-4 flex items-center text-[11px] text-[var(--color-muted)] border-t border-[var(--color-border)] bg-[var(--color-surface)]">{cap}</footer>
    </div>
  );
}
