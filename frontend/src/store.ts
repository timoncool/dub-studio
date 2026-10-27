import { create } from "zustand";
import { journalApi, projectRev, type JobKind, type JournalLine, type Project } from "./lib/api";

/**
 * A state of the open project undo or redo can write back, with the revision the window knew when it was taken:
 * the studio refuses to write it once anyone but this window saved the project after that revision.
 */
export type Snapshot = { project: Project; rev: number | undefined };

type Stage = "boot" | "offline" | "setup" | "update" | "empty" | "analyzing" | "editor" | "batch" | "multilang";
export type ExportItem = { id: string; name: string; status: "rendering" | "done" | "error"; msg: string; url?: string; pid?: string };
// строка журнала «что делает приложение»; agent — действие агента по MCP; verb/noun/target — изменение агента, словами окна
export type Activity = { t: number; text: string; kind: "work" | "done" | "error" | "agent"; verb?: string; noun?: string; target?: string };

const KEPT = 500;

const fromLine = (l: JournalLine): Activity => ({ t: l.at, text: l.text, kind: l.tone, ...(l.source === "agent" ? { verb: l.verb, noun: l.kind, target: l.target } : {}) });

/** A line the window shows goes to the server's journal too, which keeps it across restarts. */
function keep(text: string, kind: Activity["kind"]): void {
  journalApi.write(kind, text).catch((problem: unknown) => console.error("[ERROR] the journal did not take a line:", problem));
}
export type CurrentJob = { id: string; kind: JobKind; pid: string };
// Джоба, за которой следит сам редактор (экспорт, озвучка, ремикс): полоса джоб проекта показывает её с «Отменить».
export type LocalJob = { id: string; kind: JobKind; pid: string; stage: string; msg: string; ahead: number | null };
// Джоба упала на экране анализа: ждём решения пользователя (продолжить с места остановки или назад).
export type JobFailure = { pid: string; msg: string; resolve: (choice: "continue" | "back") => void };

type State = {
  stage: Stage;
  pid: string | null;
  project: Project | null;
  progress: { stage: string; msg: string; pct: number | null };
  rendered: boolean;
  rendering: boolean;                // current project's export in flight (button state; does NOT block the screen)
  exports: ExportItem[];            // non-blocking export queue shown in the bottom-right Files panel
  activities: Activity[];           // журнал «что делает приложение» — статус-строка в шапке + разворот в лог
  past: Snapshot[];                 // undo/redo history of Project snapshots
  future: Snapshot[];
  rev: number;                       // preview cache-buster: bumped on every backend-confirmed frame change
  dubRev: number;                    // dub-audio cache-buster: bumped ONLY when the dub track is re-rendered (regen/export/finished job), NOT on every edit, so live edits don't reload <audio> mid-playback
  selBlur: number | null;            // selected blur-box index — SHARED between the left list and the canvas overlay
  selTitle: number | null;           // selected title index — SHARED between the left titles list and the canvas overlay
  justAnalyzed: boolean;             // только что прошёл analyze -> редактор один раз авто-генерит дуб (чтобы сразу слушать)
  setJustAnalyzed: (b: boolean) => void;
  audioOnly: boolean;                // вход без видео -> заголовок прогресса «Анализируем аудио», не «видео»
  setAudioOnly: (b: boolean) => void;
  jobSteps: string[] | null;         // ключи шагов степпера ТЕКУЩЕЙ джобы (по конфигу запуска); null = все
  setJobSteps: (s: string[] | null) => void;
  setStage: (s: Stage) => void;
  setPid: (p: string | null) => void;
  setProject: (p: Project | null) => void;
  setProgress: (stage: string, msg: string, pct?: number | null) => void;
  setRendered: (b: boolean) => void;
  setRendering: (b: boolean) => void;
  addExport: (e: ExportItem) => void;
  updateExport: (id: string, patch: Partial<ExportItem>) => void;
  removeExport: (id: string) => void;
  pushHistory: (p: Project) => void; // snapshot the project BEFORE a mutation (for undo)
  undo: () => Snapshot | null;       // returns the snapshot to restore (PUT it with its rev) or null
  redo: () => Snapshot | null;
  bump: () => void;                  // invalidate the rendered preview frame -> <img> refetches
  bumpDub: () => void;
  setSelBlur: (i: number | null) => void;
  setSelTitle: (i: number | null) => void;
  pushActivity: (text: string, kind?: Activity["kind"]) => void;   // добавить строку в журнал
  loadJournal: () => Promise<void>;                                  // журнал сервера (переживает перезапуск)
  clearJournal: () => Promise<void>;
  resetHistory: () => void;         // проект изменили агент или другое окно: старые снимки undo затёрли бы их правки
  currentJob: CurrentJob | null;     // джоба экрана анализа (для «Отменить»)
  setCurrentJob: (j: CurrentJob | null) => void;
  queuedAhead: number | null;        // сколько джоб впереди, пока текущая в очереди
  setQueuedAhead: (n: number | null) => void;
  resumedStages: string[];           // стадии, взятые из кэша прошлого прогона (отметка в степпере)
  markResumed: (stage: string) => void;
  clearResumed: () => void;
  jobFailure: JobFailure | null;
  setJobFailure: (f: JobFailure | null) => void;
  localJobs: LocalJob[];
  putLocalJob: (j: LocalJob) => void;
  patchLocalJob: (id: string, patch: Partial<LocalJob>) => void;
  dropLocalJob: (id: string) => void;
  jobsRev: number;                   // растёт на каждом конце джобы редактора: полоса джоб перечитывает итог проекта
  /** The window over the page that names the hub's section for notices: settings, news or a library. */
  overlay: "settings" | "news" | "library" | null;
  setOverlay: (overlay: "settings" | "news" | "library" | null) => void;
};

const known = (pid: string | null): number | undefined => (pid === null ? undefined : projectRev(pid));

export const useStore = create<State>((set, get) => ({
  stage: "boot",   // при загрузке SPA сперва проверяем /setup/status; если чего-то обязательного нет -> "setup"
  pid: null,
  project: null,
  progress: { stage: "", msg: "", pct: null },
  rendered: false,
  rendering: false,
  exports: [],
  activities: [],
  past: [],
  future: [],
  rev: 0,
  dubRev: 0,
  selBlur: null,
  selTitle: null,
  justAnalyzed: false,
  setJustAnalyzed: (justAnalyzed) => set({ justAnalyzed }),
  audioOnly: false,
  setAudioOnly: (audioOnly) => set({ audioOnly }),
  jobSteps: null,
  setJobSteps: (jobSteps) => set({ jobSteps }),
  setStage: (stage) => set({ stage }),
  setPid: (pid) => set({ pid }),
  setProject: (project) => set({ project }),
  setProgress: (stage, msg, pct = null) => set((s) => {   // keep the last message on a stage-only tick; pct only during a download
    const progress = { stage, msg: msg || s.progress.msg, pct: pct ?? null };
    const text = (msg || stage || "").trim();
    if (!text) return { progress };
    const kind: Activity["kind"] = stage === "error" ? "error" : stage === "done" ? "done" : "work";
    const last = s.activities[s.activities.length - 1];
    if (last && last.text === text && last.kind === kind) return { progress };   // дедуп повторов
    keep(text, kind);
    return { progress, activities: [...s.activities, { t: Date.now(), text, kind }].slice(-KEPT) };
  }),
  setRendered: (rendered) => set({ rendered }),
  setRendering: (rendering) => set({ rendering }),
  addExport: (e) => set((s) => ({ exports: [e, ...s.exports.filter((x) => x.id !== e.id)] })),   // дедуп по id: повторный экспорт того же проекта заменяет запись, а не плодит дубли
  updateExport: (id, patch) => set((s) => ({ exports: s.exports.map((x) => (x.id === id ? { ...x, ...patch } : x)) })),
  removeExport: (id) => set((s) => ({ exports: s.exports.filter((x) => x.id !== id) })),
  pushHistory: (p) => set((s) => {
    // no-op if the snapshot matches the current head: callers may re-snapshot the same
    // baseline (e.g. a second keystroke in the same edit burst), and an unconditional
    // future:[] there would silently kill the redo stack.
    const head = s.past[s.past.length - 1];
    if (head && JSON.stringify(head.project) === JSON.stringify(p)) return {};
    return { past: [...s.past, { project: p, rev: known(s.pid) }].slice(-60), future: [] };
  }),
  undo: () => {
    const s = get(); if (!s.past.length || !s.project) return null;
    const prev = s.past[s.past.length - 1];
    set({ past: s.past.slice(0, -1), future: [{ project: s.project, rev: known(s.pid) }, ...s.future], project: prev.project });
    return prev;
  },
  redo: () => {
    const s = get(); if (!s.future.length || !s.project) return null;
    const next = s.future[0];
    set({ future: s.future.slice(1), past: [...s.past, { project: s.project, rev: known(s.pid) }], project: next.project });
    return next;
  },
  bump: () => set((s) => ({ rev: s.rev + 1 })),
  bumpDub: () => set({ dubRev: Date.now() }),
  setSelBlur: (selBlur) => set({ selBlur }),
  setSelTitle: (selTitle) => set({ selTitle }),
  pushActivity: (text, kind = "work") => set((s) => {
    const clean = (text || "").trim();
    if (!clean) return {};
    const last = s.activities[s.activities.length - 1];
    if (last && last.text === clean && last.kind === kind) return {};   // дедуп повторов
    keep(clean, kind);
    return { activities: [...s.activities, { t: Date.now(), text: clean, kind }].slice(-KEPT) };
  }),
  loadJournal: async () => {
    const lines = await journalApi.read();
    set({ activities: lines.map(fromLine) });
  },
  clearJournal: async () => {
    await journalApi.clear();
    set({ activities: [] });
  },
  currentJob: null,
  setCurrentJob: (currentJob) => set({ currentJob }),
  queuedAhead: null,
  setQueuedAhead: (queuedAhead) => set({ queuedAhead }),
  resumedStages: [],
  markResumed: (stage) => set((s) => (s.resumedStages.includes(stage) ? {} : { resumedStages: [...s.resumedStages, stage] })),
  clearResumed: () => set({ resumedStages: [] }),
  jobFailure: null,
  setJobFailure: (jobFailure) => set({ jobFailure }),
  localJobs: [],
  putLocalJob: (j) => set((s) => ({ localJobs: [...s.localJobs.filter((x) => x.id !== j.id), j] })),
  patchLocalJob: (id, patch) => set((s) => ({ localJobs: s.localJobs.map((x) => (x.id === id ? { ...x, ...patch } : x)) })),
  dropLocalJob: (id) => set((s) => ({ localJobs: s.localJobs.filter((x) => x.id !== id), jobsRev: s.jobsRev + 1 })),
  jobsRev: 0,
  overlay: null,
  setOverlay: (overlay) => set({ overlay }),
  resetHistory: () => set({ past: [], future: [] }),
}));
