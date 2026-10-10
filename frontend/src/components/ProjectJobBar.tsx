import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2, RotateCw, Square, X } from "lucide-react";
import { api, JobCancelledError, type JobKind, type JobSnapshot, type Project, type ProjectJob } from "../lib/api";
import { continueProject, isOwnJob, RESUMABLE_STATES } from "../lib/jobs";
import { useJobErrorText, useJobStateLabel } from "../lib/jobLabels";
import { STAGE_TO_STEPKEY } from "../lib/stages";
import i18n from "../lib/i18n";
import { useStore } from "../store";

type Live = { stage: string; msg: string; ahead: number | null };

const fileName = (path: string) => path.split(/[\\/]/).pop() || path;

// Джоба завершилась: свежий проект; новая озвучка -> плеер перечитывает дуб; экспорт -> готовый файл в панели.
function jobFinished(pid: string, kind: JobKind, exportId: string, exportName: string, project: Project) {
  const s = useStore.getState();
  s.setProject(project);
  if (kind === "render") {
    s.updateExport(exportId, { status: "done", msg: "", url: `${api.outputUrl(pid)}?rev=${Date.now()}` });
    s.pushActivity(`${i18n.t("compare.result")}: ${exportName}`, "done");
  }
  s.setRendered(kind === "render");
  if (kind === "render" || kind === "dub_audio") s.bumpDub();
  s.bump();
}

const ROW = "flex items-center gap-2.5 max-w-[min(640px,90vw)] rounded-lg border px-3 py-1.5 text-[12px] shadow-lg backdrop-blur";

// Идущая джоба: вид, этап или место в очереди и «Отменить». Строка пропадает, когда джоба заканчивается.
function JobRow({ id, kind, live }: { id: string; kind: JobKind; live: Live }) {
  const { t } = useTranslation();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function cancel() {
    setPending(true);
    setError(null);
    try {
      await api.cancelJob(id);
    } catch (e) {
      setPending(false);
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  const step = live.stage ? STAGE_TO_STEPKEY[live.stage] : undefined;
  const detail = live.ahead != null ? t("jobs.queuedAhead", { n: live.ahead }) : step ? t(`analyze.${step}`) : live.msg;
  return (
    <div className={`${ROW} border-[var(--color-border)] bg-[var(--color-surface)]/95`}>
      <Loader2 size={13} className="animate-spin text-[var(--color-accent)] shrink-0" />
      <span className="font-medium shrink-0">{t(`jobs.kind.${kind}`)}</span>
      <span className="min-w-0 truncate text-[var(--color-muted)]">{detail}</span>
      {error && <span className="min-w-0 truncate mono text-[11px] text-[var(--color-warn)]" title={error}>{error}</span>}
      <button onClick={cancel} disabled={pending}
        className="ml-1 shrink-0 inline-flex items-center gap-1 px-2 py-0.5 rounded-md border border-[var(--color-border)] text-[var(--color-muted)] hover:text-[var(--color-text)] hover:border-[var(--color-warn)] disabled:opacity-50 transition-colors">
        {pending ? <Loader2 size={12} className="animate-spin" /> : <Square size={11} />}{pending ? t("jobs.cancelling") : t("jobs.cancel")}
      </button>
    </div>
  );
}

// Джобы открытого проекта. Запущенные из редактора (экспорт, озвучка, ремикс) показываются, пока идут, с
// «Отменить». Джобу, которую окно не ставило (шла до перезагрузки окна, поставлена «Продолжить» или агентом),
// полоса подхватывает сама и по её концу обновляет проект. Если последняя джоба проекта прервана, упала или
// отменена — предлагает «Продолжить» с места остановки.
export default function ProjectJobBar({ pid }: { pid: string }) {
  const { t } = useTranslation();
  const stateLabel = useJobStateLabel();
  const errText = useJobErrorText();
  const localJobs = useStore((s) => s.localJobs);
  const jobsRev = useStore((s) => s.jobsRev);
  const [job, setJob] = useState<JobSnapshot | null>(null);
  const [live, setLive] = useState<Live>({ stage: "", msg: "", ahead: null });
  const [record, setRecord] = useState<ProjectJob | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [hidden, setHidden] = useState(false);
  const [reload, setReload] = useState(0);

  useEffect(() => {
    let alive = true;
    const watch = new AbortController();
    api.jobs(pid)
      .then(async (r) => {
        if (!alive) return;
        const active = r.jobs.find((j) => (j.status === "queued" || j.status === "running") && !isOwnJob(j.id)) ?? null;
        setJob(active);
        if (!active) return;
        setLive({ stage: active.stage ?? "", msg: active.msg ?? "", ahead: active.position ?? null });
        // Экспорт, подхваченный окном, — та же запись в панели файлов, что у кнопки «Экспорт».
        const exportId = `export-${pid}`;
        const project = useStore.getState().project;
        const exportName = project?.source_name || fileName(project?.meta.video || pid);
        if (active.kind === "render") useStore.getState().addExport({ id: exportId, name: exportName, status: "rendering", msg: i18n.t("common.rendering"), pid });
        try {
          await api.watchJob(active.id, (e) => {
            if (!alive) return;
            if (e.type === "queued") setLive((l) => ({ ...l, ahead: e.position ?? null }));
            else if (e.type === "progress") setLive({ stage: e.stage ?? "", msg: e.msg ?? "", ahead: null });
          }, watch.signal);
          if (!alive || isOwnJob(active.id)) return;
          jobFinished(pid, active.kind, exportId, exportName, await api.getProject(pid));
        } catch (e) {
          if (watch.signal.aborted || isOwnJob(active.id)) return;
          if (active.kind === "render") useStore.getState().updateExport(exportId, { status: "error", msg: e instanceof JobCancelledError ? i18n.t("jobs.state.cancelled") : e instanceof Error ? e.message : String(e) });
          if (alive && !(e instanceof JobCancelledError)) setError(e instanceof Error ? e.message : String(e));
        } finally {
          if (alive) {
            setJob(null);
            setReload((n) => n + 1);
          }
        }
      })
      .catch((e) => { if (alive) setError(e instanceof Error ? e.message : String(e)); });
    return () => { alive = false; watch.abort(); };
  }, [pid, reload]);

  // Итог последней джобы проекта (job.json) перечитывается после каждой джобы — и подхваченной, и из редактора.
  useEffect(() => {
    let alive = true;
    api.jobs(pid)
      .then((r) => {
        if (!alive) return;
        setRecord(r.project_job);
        setHidden(false);
      })
      .catch((e) => { if (alive) setError(e instanceof Error ? e.message : String(e)); });
    return () => { alive = false; };
  }, [pid, reload, jobsRev]);

  async function resume(rec: ProjectJob) {
    setBusy(true);
    setError(null);
    const s = useStore.getState();
    const meta = s.project?.meta;
    const audioOnly = !!meta && (meta.width <= 0 || meta.height <= 0);
    try {
      await continueProject(pid, rec.kind, audioOnly, async (id) => {
        if (rec.kind === "analyze") {
          s.setProject(await api.getProject(id));
          s.setStage("editor");
        }
        setReload((n) => n + 1);
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  const wrap = "fixed top-14 left-1/2 -translate-x-1/2 z-40 flex flex-col items-center gap-1.5";
  const own = localJobs.filter((j) => j.pid === pid);
  const picked = job && !own.some((j) => j.id === job.id) ? job : null;
  if (picked || own.length > 0) {
    return (
      <div className={wrap}>
        {picked && <JobRow key={picked.id} id={picked.id} kind={picked.kind} live={live} />}
        {own.map((j) => <JobRow key={j.id} id={j.id} kind={j.kind} live={j} />)}
      </div>
    );
  }

  if (hidden) return null;
  if (record && RESUMABLE_STATES.has(record.state)) {
    return (
      <div className={wrap}>
        <div className={`${ROW} border-[var(--color-warn)]/40 bg-[color-mix(in_oklab,var(--color-warn)_12%,var(--color-surface))]`}>
          <span className="font-medium shrink-0">{t(`jobs.kind.${record.kind}`)}</span>
          <span className="min-w-0 truncate text-[var(--color-warn)]" title={errText(record.error)}>{stateLabel(record.state, record.stage)}</span>
          {error && <span className="min-w-0 truncate mono text-[11px] text-[var(--color-warn)]" title={error}>{error}</span>}
          <button onClick={() => resume(record)} disabled={busy} title={t("jobs.continueHint")}
            className="ml-1 shrink-0 inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-[var(--color-accent)] text-[var(--color-on-accent)] font-semibold hover:opacity-90 disabled:opacity-50 transition-opacity">
            {busy ? <Loader2 size={12} className="animate-spin" /> : <RotateCw size={12} />}{t("jobs.continue")}
          </button>
          <button onClick={() => setHidden(true)} title={t("jobs.dismiss")}
            className="shrink-0 text-[var(--color-muted)] hover:text-[var(--color-text)]"><X size={14} /></button>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className={wrap}>
        <div className={`${ROW} border-[var(--color-warn)]/40 bg-[var(--color-surface)]/95`}>
          <span className="min-w-0 truncate mono text-[11px] text-[var(--color-warn)]" title={error}>{error}</span>
          <button onClick={() => setHidden(true)} title={t("jobs.dismiss")}
            className="shrink-0 text-[var(--color-muted)] hover:text-[var(--color-text)]"><X size={14} /></button>
        </div>
      </div>
    );
  }
  return null;
}
