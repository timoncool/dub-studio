import { useCallback, useEffect, useState } from "react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { ArrowRight, Check, Download, Loader2 } from "lucide-react";
import { api, SetupError, type DownloadJob, type SetupStatus, type UpdatePlan } from "../lib/api";
import { fmtBytes } from "../lib/format";
import { useDownloadErrorText } from "../lib/setupText";
import { useSetupStatus } from "../lib/useSetupStatus";
import { useStore } from "../store";
import DownloadProgress from "./DownloadProgress";

/** The update screen's plan: three groups, the totals and one button that downloads it all and then removes what is
 * no longer needed. `onDone` follows a completed update; `onLater` is given where the screen can be put off; `progress`
 * off where the page around it shows the download already. */
export function UpdatePlanView({ onDone, onLater, progress = true }: { onDone?: (s: SetupStatus) => void; onLater?: () => void; progress?: boolean }) {
  const { t } = useTranslation();
  const errText = useDownloadErrorText();
  const [plan, setPlan] = useState<UpdatePlan | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const failText = (e: unknown) => (e instanceof SetupError ? `${errText(e.code)} · ${e.detail}` : e instanceof Error ? e.message : String(e));
  const load = useCallback(() => api.setupUpdate().then(setPlan, (e: unknown) => setErr(failText(e))), []); // eslint-disable-line react-hooks/exhaustive-deps
  const onSettled = (job: DownloadJob, s: SetupStatus) => {
    if (!job.retireAfter) return;
    if (job.status === "completed") {
      useStore.getState().pushActivity(t("update.done"), "done");
      void load();
      onDone?.(s);
    } else if (job.status === "failed") {
      useStore.getState().pushActivity(`${errText(job.errorCode)} · ${job.error ?? ""}`, "error");
    }
  };
  const { status, refresh, downloading } = useSetupStatus(onSettled);
  useEffect(() => { void load(); }, [load]);

  const apply = async () => {
    setErr(null);
    setStarting(true);
    try {
      const r = await api.setupUpdateApply();
      setPlan(r.plan);
      const s = await refresh();
      if (!r.download) {
        useStore.getState().pushActivity(t("update.done"), "done");
        onDone?.(s);
      }
    } catch (e) {
      setErr(failText(e));
    } finally {
      setStarting(false);
    }
  };
  const later = () => {
    api.setupUpdateSeen().then(() => onLater?.(), (e: unknown) => setErr(failText(e)));
  };
  const resume = (ids: string[]) => {
    setErr(null);
    api.setupDownload(ids).then(() => refresh()).catch((e: unknown) => setErr(failText(e)));
  };
  const pause = () => { api.setupCancel().then(() => refresh()).catch((e: unknown) => setErr(failText(e))); };

  const active = status?.active ?? null;
  const updateJob = active?.retireAfter ? active : null;
  const busy = downloading || starting;
  const empty = plan != null && plan.updated.length + plan.added.length + plan.retired.length === 0;
  const noSpace = plan != null && status?.freeBytes != null && plan.download > status.freeBytes;
  const pct = (id: string): number | null => {
    if (!updateJob || updateJob.status !== "downloading") return null;
    const p = updateJob.parts.find((x) => x.id === id);
    return p && p.total > 0 ? (p.done / p.total) * 100 : updateJob.ids.includes(id) ? 0 : null;
  };

  const group = (label: string, rows: ReactNode[]) => rows.length > 0 && (
    <div className="rounded-xl border border-[var(--color-border)] bg-[var(--color-surface)] px-3.5 py-3">
      <div className="text-[11px] uppercase tracking-[0.14em] text-[var(--color-muted)] mb-2">{label}</div>
      <div className="space-y-2">{rows}</div>
    </div>
  );
  const line = (key: string, name: string, version: ReactNode, size: string, reason: string, progress: number | null) => (
    <div key={key} data-mcp-context={`${name}: ${reason}`}>
      <div className="flex items-center gap-2">
        <span className="text-[13px] font-semibold truncate">{name}</span>
        {version}
        <span className="ml-auto mono text-[11px] text-[var(--color-muted)] shrink-0">{size}</span>
      </div>
      <div className="text-[12px] leading-snug text-[var(--color-muted)]">{reason}</div>
      {progress != null && (
        <div className="mt-1 h-1 rounded-full bg-[var(--color-surface-2)] overflow-hidden"><div className="h-full bg-[var(--color-accent)]" style={{ width: `${progress}%` }} /></div>
      )}
    </div>
  );
  const version = (from: string | null, to: string) => (
    <span className="mono text-[10.5px] text-[var(--color-muted)] inline-flex items-center gap-1 min-w-0 truncate">
      {from && <>{from}<ArrowRight size={11} className="shrink-0" /></>}{to}
    </span>
  );

  return (
    <div className="space-y-3">
      {err && (
        <div className="rounded-lg border border-[var(--color-warn)]/40 bg-[color-mix(in_oklab,var(--color-warn)_10%,transparent)] px-3 py-2 text-[13px] text-[var(--color-warn)]">
          <span className="font-semibold">{t("setup.error")}</span> · <span className="mono text-[11px] break-words">{err}</span>
        </div>
      )}
      {progress && active && active.status !== "completed" && <DownloadProgress job={active} onPause={pause} onResume={resume} />}
      {!plan && !err && <Loader2 size={18} className="animate-spin text-[var(--color-muted)]" />}
      {empty && <div className="text-[13px] text-[var(--color-muted)]">{t("update.nothing")}</div>}
      {plan && group(t("update.updated"), plan.updated.map((r) => line(r.id, r.name, version(r.from, r.to), fmtBytes(r.download), r.reason, pct(r.id))))}
      {plan && group(t("update.added"), plan.added.map((r) => line(r.id, r.name, version(null, r.to), fmtBytes(r.download), r.reason, pct(r.id))))}
      {plan && group(t("update.retired"), plan.retired.map((g) => line(g.path, g.path, null, g.size > 0 ? `−${fmtBytes(g.size)}` : "", g.reason, null)))}
      {plan && !empty && (
        <div className="flex flex-wrap items-center gap-3 pt-1">
          <button onClick={() => { void apply(); }} disabled={busy || noSpace}
            title={noSpace && status?.freeBytes != null ? t("downloads.noSpace", { need: fmtBytes(plan.download), free: fmtBytes(status.freeBytes) }) : undefined}
            className="inline-flex items-center gap-2 px-4 py-2.5 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-sm font-semibold disabled:opacity-40 hover:brightness-105">
            {busy ? <Loader2 size={16} className="animate-spin" /> : plan.download > 0 ? <Download size={16} /> : <Check size={16} />}
            {t("update.apply")}
          </button>
          <span className="mono text-[12px] text-[var(--color-muted)]">
            {[plan.download > 0 ? t("update.download", { size: fmtBytes(plan.download) }) : "", plan.freed > 0 ? t("update.freed", { size: fmtBytes(plan.freed) }) : ""].filter(Boolean).join(" · ")}
          </span>
          {onLater && !busy && (
            <button onClick={later} className="ml-auto px-3 py-2.5 rounded-lg border border-[var(--color-border)] text-sm text-[var(--color-muted)] hover:text-[var(--color-text)]">{t("update.later")}</button>
          )}
        </div>
      )}
    </div>
  );
}

/** The screen at the first start of a release whose plan is not empty, built like the setup screen. */
export default function UpdateScreen() {
  const { t } = useTranslation();
  const setStage = useStore((s) => s.setStage);
  const leave = (s?: SetupStatus) => {
    if (s) { setStage(s.ready ? "empty" : "setup"); return; }
    api.setupStatus().then((st) => setStage(st.ready ? "empty" : "setup"), (e: unknown) => setStage(e instanceof TypeError ? "offline" : "empty"));
  };
  return (
    <div className="flex-1 min-h-0 overflow-y-auto grid place-items-center px-6 py-10">
      <div className="w-full max-w-2xl">
        <div className="mono text-[11px] tracking-[0.2em] text-[var(--color-muted)] flex items-center gap-2">
          <span className="w-1.5 h-1.5 rounded-full bg-[var(--color-accent)] shadow-[0_0_8px_var(--color-accent)]" />{t("update.kicker")}
        </div>
        <h1 className="mt-4 text-2xl lg:text-[28px] leading-tight font-extrabold tracking-tight">{t("update.title")}</h1>
        <p className="mt-3 mb-4 text-[14px] leading-relaxed text-[var(--color-muted)]">{t("update.subtitle")}</p>
        <UpdatePlanView onDone={leave} onLater={() => leave()} />
      </div>
    </div>
  );
}
