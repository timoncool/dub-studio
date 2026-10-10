import i18n from "./i18n";
import type en from "../locales/en.json";
// Dub Studio API client — talks to the single-worker FastAPI backend over the dub-engine.
// dev: Vite (5173) -> dub-server (8793, DUB_STUDIO_PORT). desktop/portable: the server serves the SPA
// itself, so calls are same-origin (""). VITE_API overrides both.
export const BASE = (import.meta.env.VITE_API as string | undefined) ?? (import.meta.env.DEV ? "http://127.0.0.1:8793" : "");

/** This window's mark: the studio tells the changes the window made itself apart from an agent's or another window's. */
export const WINDOW_ID = crypto.randomUUID();

// Ревизия копии проекта в окне: её называет заголовок ответов, которые и есть проект (GET, PATCH, PUT).
const revisions = new Map<string, number>();
const REV_HEADER = "x-project-rev";
const PROJECT_ROUTE = /^\/projects\/([A-Za-z0-9]+)(\/|$)/;

/** The revision the window's copy of a project is at, as far as it knows. */
export function projectRev(pid: string): number | undefined {
  return revisions.get(pid);
}

/**
 * Every request of this module goes through here instead of the global fetch: it carries the window's mark,
 * a whole-project PUT carries the revision its state was taken at - the one it names itself, or the latest the
 * window knows (the studio refuses it when someone else saved the project after that revision) - and the
 * revision an answer shows is remembered.
 */
function fetch(input: string, init: RequestInit = {}): Promise<Response> {
  const headers = new Headers(init.headers);
  headers.set("x-dub-window", WINDOW_ID);
  const path = new URL(input, window.location.href).pathname;
  const pid = PROJECT_ROUTE.exec(path)?.[1];
  const known = pid === undefined ? undefined : revisions.get(pid);
  if (pid !== undefined && known !== undefined && init.method === "PUT" && path === `/projects/${pid}` && !headers.has(REV_HEADER)) headers.set(REV_HEADER, String(known));
  return globalThis.fetch(input, { ...init, headers }).then((r) => {
    const rev = r.headers.get(REV_HEADER);
    if (pid !== undefined && r.ok && rev !== null) revisions.set(pid, Number(rev));
    return r;
  });
}

export type SubStyle = {
  color: string; outline: string; italic: boolean; bold: boolean; uppercase: boolean;
  font?: string | null; scene_color?: string | null; scene_flat: boolean;
  n_lines?: number | null; align: string; size_px?: number | null; outline_w?: number | null; shadow_dir?: number | null;
};
// Укладка перевода в слот (вычисляет сервер): прогноз по темпу голоса и отчёт последнего рендера этого текста.
export type FitVerdict = "fits" | "tight" | "impossible";
export type FitRendered = { needed: number; cap: number; eff_cap: number; raw: number; slot: number; dur: number | null; over: boolean };
export type Fit = {
  est: number; slot: number; ratio: number; verdict: FitVerdict; calibrated: boolean; cps: number; cap: number;
  eff_cap: number; over: boolean; rendered: FitRendered | null;
};
// История дублей фразы: сводка у реплики и полный список (GET /segments/{id}/takes).
export type TakesSummary = { count: number; active: number | null; pinned: number | null };
export type TakeSource = "synth" | "multitake" | "qc" | "shorten";
export type Take = {
  n: number; text: string; text_matches: boolean; dur: number; qc: number | null; source: TakeSource;
  voice: string; reference: string; params: string; created: number; file: string;
};
export type Takes = { id: string; active: number | null; pinned: number | null; takes: Take[] };
export type ShortenResult = {
  shortened: { id: string; from: string; to: string }[]; rejected: { id: string; reason: string }[];
  failed: { id: string; error: string }[]; unpinned: string[];
};
export type Segment = {
  id: string; start: number; end: number; speaker?: string | null;
  src_text: string; tgt_text: string; voice?: string | null; dirty: boolean; hidden?: boolean; keep_original?: boolean;
  fit?: Fit | null; takes?: TakesSummary | null; shortened?: { from: string; to: string } | null;
  // что услышит озвучка, если отличается от tgt_text (без тегов звуков, с произношением глоссария); почему фраза не озвучивается
  tts_text?: string; tts_skip?: TtsSkip;
};
// Двуязычные субтитры (subs.mode = bilingual): порядок строк и вид второй строки (null — как у основной).
export type Bilingual = {
  order: "translation_top" | "original_top";
  secondary: { size_pct: number; color: string | null; opacity: number | null };
};
export type TtsSkip = "sound_only" | "no_words";
// Запись глоссария: перевод или keep (не переводить), произношение для озвучки, как ASR ошибается в термине.
export type GlossaryEntry = {
  term: string; translation: string; keep: boolean; pronunciation: string; asr_fix: string[]; note: string;
  source: "manual" | "auto" | "series"; lang: string;
};
export type ProjectGlossary = { entries: GlossaryEntry[]; tgt_lang: string; casting_ref: string; stale: boolean };
// Тело PUT глоссария: весь список или TSV; merge — влить в имеющиеся (присланное главнее).
export type GlossaryPut = { entries: GlossaryEntry[]; merge?: boolean } | { tsv: string; merge?: boolean; lang?: string };
export type BlurBox = { x: number; y: number; w: number; h: number; t0: number; t1: number; hidden?: boolean; fill?: string | null };
export type Title = {
  text: string; tgt: string; bbox?: number[] | null; color?: string | null; bg?: string | null;
  font?: string | null; italic: boolean; align: string; start: number; end: number;
  lh?: number | null; solid: boolean; bold: boolean; size_px?: number | null; outline?: string | null;
  outline_w?: number | null; shadow_dir?: number | null; uppercase?: boolean;
};
export type Project = {
  meta: { video: string; duration: number; width: number; height: number; fps: number; src_codec: string };
  mode: string; tgt_lang: string;
  audio: { keep_music: boolean; voice: { mode: string; name?: string | null }; rewrite?: string | null; gain_db?: number; loudness_normalize?: boolean; voiceover_gain_db?: number; translate_style?: string; keep_original_track?: boolean; container?: string };
  segments: Segment[];
  subs: { mode: string; burn?: boolean; bilingual: Bilingual };
  captions: {
    sub_style?: SubStyle | null; sub_y?: number | null; overrides: { seg_id: string; text?: string | null }[];
    titles: Title[]; brands: unknown[]; blur_boxes: BlurBox[]; preset: Record<string, unknown>;
  };
  render: { burn_cq: number; blur_sigma: number; blur: boolean; codec: string };
  work_dir?: string | null;
};
// Вид, состояние и код ошибки джобы проекта: ровно ключи jobs.kind/state/error локалей (подписи в окне).
export type JobKind = keyof typeof en.jobs.kind;
export type JobState = keyof typeof en.jobs.state;
export type JobErrorCode = keyof typeof en.jobs.error;
export type JobErrorInfo = { code: JobErrorCode; text: string };
export type ProjectSummary = {
  pid: string; video: string; tgt_lang: string; mode: string;
  width: number; height: number; duration: number; segments: number;
  audio_only: boolean; mtime: number; done: boolean;
  // последняя джоба проекта (job.json): прерванную/упавшую можно продолжить с места остановки
  job_kind: JobKind | null; job_state: JobState | null; job_stage: string | null; job_error: JobErrorInfo | null;
};
// Снапшот джобы из очереди сервера (GET /jobs, GET /jobs/{id}).
export type JobSnapshot = {
  id: string; kind: JobKind; pid: string | null;
  status: "queued" | "running" | "done" | "error" | "cancelled";
  stage: string | null; msg: string | null; pct: number | null; position?: number | null;
  result?: unknown; error?: string;
};
// job.json проекта: последняя джоба с аргументами для «Продолжить».
export type ProjectJob = {
  kind: JobKind; args: Record<string, unknown>; state: JobState; stage: string;
  error: JobErrorInfo | null; job_id: string; resumes: number; started_at: number; updated_at: number;
};
// Джоба отменена пользователем — не ошибка, а отдельный исход (watchJob отклоняется этим классом).
export class JobCancelledError extends Error {
  constructor() { super("job cancelled"); this.name = "JobCancelledError"; }
}
// 409 при постановке: по проекту уже идёт джоба того же класса (или любая — для resume/удаления).
export class JobConflictError extends Error {
  readonly jobId: string;
  readonly kind: JobKind;
  constructor(jobId: string, kind: JobKind) {
    super(i18n.t("jobs.busy", { kind: i18n.t(`jobs.kind.${kind}`) }));
    this.name = "JobConflictError";
    this.jobId = jobId;
    this.kind = kind;
  }
}
// Итог анализа: настройки стартового флоу, применённые сервером в конце анализа (голоса из библиотеки).
export type VoiceSlotsOutcome = { assigned: number } | { error: "missing_voices"; names: string[] } | { error: "no_vocals" };
export type AnalyzeResult = { project_id: string; output: string; post: { voice_slots?: VoiceSlotsOutcome } };
// Настройки, которые сервер кладёт на проект в конце анализа (и повторяет при «Продолжить»).
export type AnalyzePost = { voGain?: number; subBlur?: boolean; keepOriginal?: { container: string }; voiceSlots?: { male: string[]; female: string[] } };
const analyzePostQuery = (p: AnalyzePost): string =>
  (p.voGain != null ? `&vo_gain=${p.voGain}` : "")
  + (p.subBlur != null ? `&sub_blur=${p.subBlur ? 1 : 0}` : "")
  + (p.keepOriginal ? `&keep_original=1&container=${encodeURIComponent(p.keepOriginal.container)}` : "")
  + (p.voiceSlots ? `&voice_slots=${encodeURIComponent(JSON.stringify(p.voiceSlots))}` : "");
// Подключённый к MCP-серверу агент: последний вызов и сколько секунд назад. window_open появится с мостом окна.
export type McpStatus = { agent_connected: boolean; agent_last_call: string | null; agent_seconds_ago: number | null; agent_calls: number; window_open?: boolean };
export type ModelStack = { asr: string; llm: string; vision: string; tts: string };
// Выбор active.json: строковые слоты + флаги секретов. Ключ OpenRouter и пароль прокси сервер не отдаёт.
export type Selection = { [slot: string]: string | boolean | undefined; or_key_set?: boolean; proxy_password_set?: boolean };
// Строковый слот выбора (флаги и отсутствующие слоты -> undefined).
export const slot = (sel: Selection | undefined, key: string): string | undefined => {
  const v = sel?.[key];
  return typeof v === "string" ? v : undefined;
};
export type Capabilities = {
  device: string; tts_quant: string; asr_model: string; ffmpeg: boolean;
  languages: string[]; voice_modes: string[]; models?: ModelStack;
  // Выбор ASR-движка (active.json): движок parakeet|whisper + модель/квант Whisper.
  selection?: Selection;
  asr_engines?: string[]; whisper_models?: string[]; whisper_computes?: string[];
  // Видимые лимиты RAM (настройки): prefill-батч Gemma + длина реф-клипа клона.
  llama_ubatches?: string[]; higgs_ref_secs_opts?: string[];
};
export type JobEvent = { type: "queued" | "running" | "progress" | "done" | "error" | "cancelled"; stage?: string; pct?: number; msg?: string; result?: unknown; error?: string; component?: string; downloaded?: number; total?: number; parts?: { component: string; pct: number }[]; position?: number; resumed?: boolean };

// Кастинг персонажей (#115): бэк детектит лица (SCRFD)+эмбеддинги (LVFace)+active-speaker (LR-ASD),
// кластеризует в персонажей. GET отдаёт список; POST сохраняет имя/заметку о речи/голос дубляжа.
// speaker_ids — какие диаризованные спикеры слились в этого персонажа; sample_frame_url — кадр-аватар.
export type Character = {
  id: string; name: string; gender: string; voice: string | null;
  speech_note: string;   // манера речи/характер (уходит в translate_style); round-trip чтобы Apply не стирал перенесённое
  speaker_ids: string[]; sample_frame_url: string | null; line_count: number;   // null -> нет кадра (закадровый), фронт рисует инициал
  voice_sample_url: string | null;   // проигрываемый wav образца голоса (null -> нет образца, кнопки ▶ нет)
};

// «Первый запуск»: статус внешних компонентов (модели/движки/системные библиотеки) + автозакачка.
export type SetupComponent = {
  id: string; name: string; purpose: string;
  requirement: "required" | "recommended" | "optional";
  delivery: "download" | "bundled" | "external";
  size: number; installed: boolean; bytesOnDisk: number;
  spaceNeeded: number;   // место на томе моделей под докачку (остаток + распаковка архивов)
  missing: string[]; detail?: string | null; externalUrl?: string | null; vram?: number; fitsVram?: boolean | null;
};
// Фоновая закачка (мимо GPU-очереди): её состояние приходит в /setup/status.active, опрос вместо SSE.
export type DownloadStatus = "downloading" | "completed" | "paused" | "interrupted" | "failed";
export type DownloadPhase = "" | "waiting" | "verify" | "download" | "extract";
export type DownloadJob = {
  id: string; ids: string[]; status: DownloadStatus; phase: DownloadPhase;
  downloaded: number; total: number; speedBps: number; waitingS: number;
  parts: { id: string; done: number; total: number }[];
  errorCode?: string | null; error?: string | null; startedAt: number; updatedAt: number;
};
// Видеокарта против требований CUDA 13 (драйвер 580+, compute capability 7.5+).
export type GpuReason = "no_nvidia" | "no_device" | "cuda_init" | "driver_old" | "gpu_old";
export type GpuReport = {
  nvidia: boolean; name?: string | null; driverVersion?: string | null; cudaDriver?: string | null;
  compute?: string | null; cuda13Ok: boolean; reason?: GpuReason | null; minDriver: number; minCompute: string;
};
export type SetupStatus = {
  components: SetupComponent[]; ready: boolean;
  downloadPending: number; driverOk: boolean; llamaBuild: string;
  modelsDir: string; freeBytes: number | null; gpu: GpuReport; active: DownloadJob | null;
};
export type ImportResult = { imported: string[]; files: number; errors: string[]; status: SetupStatus };

// Отказ маршрутов закачки/удаления: код для текста в UI + подробность для журнала.
export class SetupError extends Error {
  code: string;
  detail: string;
  constructor(code: string, detail: string) {
    super(detail || code);
    this.code = code;
    this.detail = detail;
  }
}

export type HwSnapshot = {
  gpuName: string; totalVram: number; usedVram: number; freeVram: number;
  gpuUtilization: number; temperature: number; powerDraw: number; powerLimit: number;
  processRam: number; totalRam: number; usedRam: number; message: string;
};

async function j<T>(r: Response): Promise<T> {
  if (r.status === 409) {
    const body = await r.text();
    const conflict = parseConflict(body);
    if (conflict) throw new JobConflictError(conflict.job_id, conflict.kind);
    throw new Error(`409 ${body}`);
  }
  if (!r.ok) throw new Error(`${r.status} ${await r.text()}`);
  return r.json() as Promise<T>;
}
function parseConflict(body: string): { job_id: string; kind: JobKind } | null {
  if (!body.startsWith("{")) return null;
  const v = JSON.parse(body) as { error?: string; job_id?: string; kind?: JobKind };
  return (v.error === "job_conflict" || v.error === "project_busy") && v.job_id && v.kind ? { job_id: v.job_id, kind: v.kind } : null;
}

// serialize mutating PATCHes: each returns the full Project, so overlapping requests would race to setProject
// (last response wins) and could clobber an un-persisted edit. Chaining keeps them ordered; PATCH itself is a
// cheap JSON write (the heavy re-render rides the preview <img>, which the GPU worker serializes separately).
let _patchChain: Promise<unknown> = Promise.resolve();
// Общий заголовок JSON-POST/PATCH + сериализация правок в одну очередь (putProject/patch не гонятся).
const JSON_HEADERS = { "Content-Type": "application/json" };
function _chain<T>(run: () => Promise<T>): Promise<T> {
  _patchChain = _patchChain.then(run, run);
  return _patchChain as Promise<T>;
}
/** Resolves once the window's queued edits of projects have been answered. */
export const editsSettled = (): Promise<void> => _patchChain.then(() => undefined, () => undefined);

// Общие обёртки: GET/POST c JSON-телом -> j<T>. Убирают повтор fetch+headers+JSON.stringify.
const getJson = <T>(path: string): Promise<T> => fetch(`${BASE}${path}`).then(j<T>);
const postJson = <T>(path: string, body: unknown): Promise<T> =>
  fetch(`${BASE}${path}`, { method: "POST", headers: JSON_HEADERS, body: JSON.stringify(body) }).then(j<T>);
// POST маршрутов /setup/*: отказ приходит {code, detail} -> SetupError (тело не JSON — код по HTTP-статусу).
async function setupPost<T>(path: string, body: unknown): Promise<T> {
  const r = await fetch(`${BASE}${path}`, { method: "POST", headers: JSON_HEADERS, body: JSON.stringify(body) });
  if (r.ok) return r.json() as Promise<T>;
  const text = await r.text();
  const parsed = ((): { code?: string; detail?: string } => { try { return JSON.parse(text); } catch { return {}; } })();
  throw new SetupError(parsed.code ?? `http_${r.status}`, parsed.detail ?? text);
}

// Ошибка ручки с кодом ({error, detail}): текст для окна выбирается по коду через t().
export class ApiError extends Error {
  code: string;
  detail: string;
  args: Record<string, unknown>;
  constructor(code: string, detail: string, args: Record<string, unknown> = {}) {
    super(detail ? `${code}: ${detail}` : code);
    this.code = code;
    this.detail = detail;
    this.args = args;
  }
}
async function coded<T>(r: Response): Promise<T> {
  if (r.ok) return r.json() as Promise<T>;
  const text = await r.text();
  let body: { error?: unknown; detail?: unknown; args?: unknown } | null;
  try { body = JSON.parse(text) as { error?: unknown; detail?: unknown; args?: unknown }; } catch { body = null; }
  if (body && typeof body.error === "string") {
    const args = body.args && typeof body.args === "object" && !Array.isArray(body.args) ? (body.args as Record<string, unknown>) : {};
    throw new ApiError(body.error, typeof body.detail === "string" ? body.detail : "", args);
  }
  throw new ApiError(`http_${r.status}`, text);
}
const sendCoded = <T>(method: "POST" | "PUT" | "DELETE", path: string, body?: unknown): Promise<T> =>
  fetch(`${BASE}${path}`, body === undefined ? { method } : { method, headers: JSON_HEADERS, body: JSON.stringify(body) }).then(coded<T>);
const getCoded = <T>(path: string): Promise<T> => fetch(`${BASE}${path}`).then(coded<T>);
const postCoded = <T>(path: string, body: unknown): Promise<T> =>
  fetch(`${BASE}${path}`, { method: "POST", headers: JSON_HEADERS, body: JSON.stringify(body) }).then(coded<T>);

// Видео по ссылке (yt-dlp): проба, загрузка в новый проект мимо очереди джоб, сам инструмент.
export type UrlQuality = "best" | "1080" | "720" | "480" | "audio";
export type UrlTrack = { lang: string; name: string | null; formats: string[] };
export type UrlProbe = {
  url: string; title: string; duration: number | null; uploader: string | null; extractor: string | null;
  thumbnail: string | null; thumbnail_data: string | null; thumbnail_error: string | null;
  max_height: number | null; has_video: boolean; has_audio: boolean; qualities: UrlQuality[];
  subtitles: UrlTrack[]; auto_subtitles: UrlTrack[]; expected_bytes: number | null; tool_version: string;
};
export type UrlFetchStatus = "downloading" | "completed" | "failed" | "cancelled" | "interrupted";
export type UrlFetch = {
  id: string; url: string; quality: UrlQuality; subsLang: string | null; cookies: boolean;
  status: UrlFetchStatus; phase: string; title: string | null; duration: number | null;
  downloaded: number; total: number | null; speedBps: number; etaS: number | null;
  pid: string | null; subsImported: boolean; warning: string | null; warningDetail: string | null;
  errorCode: string | null; error: string | null; hint: string | null; toolVersion: string | null;
  startedAt: number; updatedAt: number;
};
export type UrlTool = {
  installed: boolean; version: string; pinnedVersion: string; updated: boolean; latest: string | null;
  checkedAt: number; updateAvailable: boolean; updating: boolean; lastError: string | null; lastErrorCode: string | null;
};
export const urlApi = {
  // cookiesText — содержимое cookies.txt: окно выбирает файл, а путь к нему браузер не отдаёт.
  probe: (url: string, cookiesText: string | null) => postCoded<UrlProbe>("/url/probe", { url, cookies_text: cookiesText }),
  start: (req: { url: string; quality: UrlQuality; subs_lang: string | null; cookies_text: string | null }) => postCoded<{ fetch: UrlFetch }>("/projects/from_url", req),
  list: () => getCoded<{ fetches: UrlFetch[] }>("/url/fetches"),
  get: (id: string) => getCoded<UrlFetch>(`/url/fetches/${encodeURIComponent(id)}`),
  cancel: (id: string) => postCoded<{ fetch: UrlFetch }>(`/url/fetches/${encodeURIComponent(id)}/cancel`, {}),
  resume: (id: string) => postCoded<{ fetch: UrlFetch }>(`/url/fetches/${encodeURIComponent(id)}/resume`, {}),
  forget: (id: string) => sendCoded<{ ok: boolean }>("DELETE", `/url/fetches/${encodeURIComponent(id)}`),
  tool: () => getCoded<UrlTool>("/url/tool"),
  updateTool: () => postCoded<{ started: boolean; tool: UrlTool }>("/url/tool/update", {}),
};

export type OpenRouterSettings = { configured: boolean; source: "environment" | "local_store" | null; environment_variable: string };
// Прокси: режим (как в Windows / свой / без прокси), тип для адреса без схемы, адрес без пароля и problem —
// почему сохранённый свой адрес не читается.
export type ProxyMode = "system" | "custom" | "off";
export type ProxyKind = "http" | "https" | "socks5" | "socks4";
export type ProxySettings = { mode: ProxyMode; kind: ProxyKind; on: boolean; url: string; password_set: boolean; problem: string | null };
export type ProxyProbe = { ok: boolean; hf?: boolean; openrouter?: boolean; hf_error?: string | null; openrouter_error?: string | null; error?: string };
// Модель каталога OpenRouter: цены — строки USD за единицу (токен), как у OpenRouter.
export type OrPricing = { prompt?: string; completion?: string; image?: string; audio?: string; request?: string };
export type OrModel = { id: string; name: string; context_length?: number | null; pricing?: OrPricing | null; input_modalities?: string[]; voices?: string[] };
export type OrModelKind = "llm" | "vision" | "tts" | "asr";
export type OrCatalog = { refreshed_at: number; total: number; counts: Record<OrModelKind, number> };
// Провайдер перевода/vision: своя Gemma, локальный OpenAI-совместимый сервер или OpenRouter. Сервер отдаёт
// действующий провайдер в llm_provider/vision_provider, даже если он выбран прежними флагами or_*_on.
export type LlmProviderKind = "local" | "server" | "openrouter";
export const llmProviderOf = (sel: Selection | undefined, stage: "llm" | "vision"): LlmProviderKind => {
  const v = slot(sel, stage === "llm" ? "llm_provider" : "vision_provider");
  return v === "server" || v === "openrouter" ? v : "local";
};

export const api = {
  capabilities: () => getJson<Capabilities>("/engine/capabilities"),
  setupStatus: () => getJson<SetupStatus>("/setup/status"),
  setupDownload: (ids: string[]) => setupPost<{ download: DownloadJob }>("/setup/download", { ids }),
  setupCancel: () => setupPost<{ paused: boolean }>("/setup/cancel", {}),   // пауза: скачанное остаётся, «Продолжить» докачивает
  setupRemove: (ids: string[]) => setupPost<{ removed: string[]; freedBytes: number; errors: string[]; status: SetupStatus }>("/setup/remove", { ids }),
  setupOpenModels: () => setupPost<{ path: string }>("/setup/open-models", {}),
  hwSnapshot: () => getJson<HwSnapshot>("/hw/snapshot"),
  setupBrowse: (id?: string) => setupPost<ImportResult & { picked: boolean }>("/setup/browse", id ? { id } : {}),
  fonts: () => getJson<{ fonts: Record<string, string> }>("/fonts"),
  setOpts: (edit: Partial<ModelStack>) =>
    fetch(`${BASE}/engine/opts`, { method: "PATCH", headers: JSON_HEADERS, body: JSON.stringify(edit) }).then(j<{ models: ModelStack }>),
  // Сделать вариант модели (квант) активным: id компонента настроек -> пишет models/active.json на бэке.
  selectModel: (id: string) => postJson<Selection>("/engine/select", { id }),
  // Прямая установка слота выбора (движок/модель/квант ASR) без скачивания: {key,value} -> active.json.
  setSelection: (key: string, value: string) => postJson<Selection>("/engine/select", { key, value }),
  // Облачные модели (OpenRouter): проверка ключа + фильтрованный каталог по модальности (llm/vision/tts).
  openrouterVerify: (key: string) => postJson<{ ok: boolean; data?: { label?: string; limit?: number; usage?: number }; error?: unknown }>("/engine/openrouter/verify", { key }),
  // Ключ OpenRouter: сервер отдаёт только «задан ли» и источник; PUT сначала проверяет ключ в OpenRouter.
  openrouterSettings: () => getJson<OpenRouterSettings>("/engine/openrouter/settings"),
  googleSettings: () => getJson<OpenRouterSettings>("/engine/google/settings"),
  saveGoogleKey: (apiKey: string) => sendCoded<OpenRouterSettings>("PUT", "/engine/google/settings", { api_key: apiKey }),
  deleteGoogleKey: () => sendCoded<OpenRouterSettings>("DELETE", "/engine/google/settings"),
  googleModels: () => getJson<{models: {name:string;displayName?:string;supportedGenerationMethods?:string[]}[]}>("/engine/google/models"),
  saveOpenrouterKey: (apiKey: string) => sendCoded<OpenRouterSettings>("PUT", "/engine/openrouter/settings", { api_key: apiKey }),
  deleteOpenrouterKey: () => sendCoded<OpenRouterSettings>("DELETE", "/engine/openrouter/settings"),
  // Прокси: адрес без пароля + флаг. password: нет поля — оставить сохранённый, null — удалить, строка — заменить.
  proxySettings: () => getJson<ProxySettings>("/engine/proxy/settings"),
  saveProxy: (form: { mode?: ProxyMode; kind?: ProxyKind; url?: string; password?: string | null }) => sendCoded<ProxySettings>("PUT", "/engine/proxy/settings", form),
  // Каталог OpenRouter (кэш на сервере, ключ не нужен): модели стадии, сводка, обновление из сети.
  openrouterModels: (kind: OrModelKind) => getJson<{ models: OrModel[]; refreshed_at: number }>(`/engine/openrouter/models?kind=${kind}`),
  openrouterCatalog: () => getJson<OrCatalog>("/engine/openrouter/catalog"),
  refreshOpenrouterCatalog: () => postJson<OrCatalog>("/engine/openrouter/catalog/refresh", {}),
  // Локальный OpenAI-совместимый сервер: модели по адресу (через сервер студии) и ключ (только «задан ли»).
  // Ключ принадлежит адресу, для которого его сохранили: на другой адрес сервер студии его не отправит.
  serverModels: (url: string) => getCoded<{ models: string[] }>(`/engine/server/models?url=${encodeURIComponent(url)}`),
  serverKey: (url: string) => getJson<{ configured: boolean }>(`/engine/server/key?url=${encodeURIComponent(url)}`),
  saveServerKey: (apiKey: string, url: string) => sendCoded<{ configured: boolean }>("PUT", "/engine/server/key", { api_key: apiKey, url }),
  deleteServerKey: (url: string) => sendCoded<{ configured: boolean }>("DELETE", `/engine/server/key?url=${encodeURIComponent(url)}`),
  // Голоса TTS-модели с полом/возрастом/русским (встроенный справочник) — для дропдауна + автокастинга.
  openrouterVoices: (model: string) => getJson<{ voices: { name: string; gender: string; age: string; ru: boolean }[]; supportsRussian: boolean | null }>(`/engine/openrouter/voices?model=${encodeURIComponent(model)}`),
  // Прокси: проверить связность до HF (закачка моделей) и OpenRouter при таком режиме/адресе — до сохранения.
  proxyTest: (form: { mode: ProxyMode; kind: ProxyKind; url: string; password?: string }) => postJson<ProxyProbe>("/engine/proxy/test", form),
  // Пресеты железа: список + детект GPU/VRAM + рекомендация; применение пишет кванты/облако в active.json.
  hwPresets: () => getJson<{ presets: { id: string; title: string; subtitle: string }[]; hardware: { gpuName: string; totalVramGb: number; totalRamGb: number; hasGpu: boolean; recommended: string; reason: string } }>("/engine/presets"),
  applyPreset: (id: string) => postJson<{ ok: boolean; id: string; applied: { key: string; value: string }[] }>("/engine/preset", { id }),
  voices: () => getJson<{ voices: string[] }>("/voices"),
  // Дефолты запуска дубляжа (форма стартового экрана) хранит сервис: одни на все окна и агента.
  launchDefaults: () => getJson<LaunchDefaultsState>("/settings/launch"),
  saveLaunchDefaults: (patch: Partial<LaunchDefaults>) =>
    fetch(`${BASE}/settings/launch`, { method: "PATCH", headers: JSON_HEADERS, body: JSON.stringify(patch) }).then(j<LaunchDefaultsState>),
  appPaths: () => getJson<AppPaths>("/app/paths"),
  // Отвечает ли сервис вообще: любой HTTP-ответ — да; false — только сетевая ошибка (на порту никого).
  serverReachable: () => fetch(`${BASE}/health`, { cache: "no-store" }).then(() => true, (e: unknown) => {
    if (e instanceof TypeError) return false;
    throw e;
  }),
  recordDevices: () => getJson<{ devices: string[] }>("/record/devices"),
  recordLevel: () => getJson<{ level: number }>("/record/level"),
  recordStart: (name: string, device?: string) => postJson<{ ok: boolean; name?: string; error?: string }>("/record/start", { name, device }),
  recordStop: () => fetch(`${BASE}/record/stop`, { method: "POST" }).then(j<{ name: string | null; voices: string[] }>),
  voicesDownloadPack: () => fetch(`${BASE}/voices/download-pack`, { method: "POST" }).then(j<{ job_id: string }>),
  voicesCatalog: () => getCoded<{ voices: { name: string; gender: string; url: string }[] }>("/voices/catalog"),
  voicesGet: (name: string) => postJson<{ ok: boolean; voices?: string[]; error?: string }>("/voices/get", { name }),
  voiceSampleUrl: (name: string) => `${BASE}/voices/sample?name=${encodeURIComponent(name)}`,   // прослушка выбранного голоса (<audio>)
  voicesRename: (from: string, to: string) => postJson<{ voices: string[] }>("/voices/rename", { from, to }),
  voicesDelete: (name: string) => postJson<{ voices: string[] }>("/voices/delete", { name }),
  speakerVoice: (pid: string, speaker: string, name: string) => sendCoded<{ ok: boolean; name: string; voices: string[] }>("POST", `/projects/${pid}/speaker-voice`, { speaker, name }),
  // Слоты голосов из библиотеки (#114): раздать голоса по спикерам по полу/приоритету. Пустые списки -> клон.
  voiceSlots: (pid: string, slots: { male: string[]; female: string[] }) =>
    postJson<{ ok: boolean; speakers: Record<string, { voice: string | null; gender: string | null; f0: number | null }> }>(`/projects/${pid}/voice-slots`, slots),
  presets: () => getJson<{ presets: Record<string, Record<string, unknown>>; reveals: string[] }>("/presets"),
  createProject: (file: File, subs?: File | null) => {
    const fd = new FormData(); fd.append("file", file);
    if (subs) fd.append("subs", subs);   // готовые субтитры (SRT/ASS) -> analyze возьмёт текст+тайминг вместо ASR
    return fetch(`${BASE}/projects`, { method: "POST", body: fd }).then(j<{ project_id: string; imported_subs?: boolean }>);
  },
  analyze: (pid: string, tgt_lang: string, mode = "auto", src_lang = "auto", subs = "auto", rewrite = "", burn = true, detect = true, importTranslated = false, translateStyle = "", casting = false, castingRef = "", contentType = "auto", alignSubs = false, post: AnalyzePost = {}, speakerCount = 0) =>
    fetch(`${BASE}/projects/${pid}/analyze?tgt_lang=${tgt_lang}&mode=${mode}&src_lang=${src_lang}&subs=${subs}&rewrite=${encodeURIComponent(rewrite)}&burn=${burn ? 1 : 0}&detect=${detect ? 1 : 0}&import_translated=${importTranslated ? 1 : 0}&translate_style=${encodeURIComponent(translateStyle)}&casting=${casting ? 1 : 0}&casting_ref=${encodeURIComponent(castingRef)}&content_type=${encodeURIComponent(contentType)}&align_subs=${alignSubs ? 1 : 0}&speaker_count=${speakerCount}${analyzePostQuery(post)}`, { method: "POST" }).then(j<{ job_id: string }>),
  // Кастинг персонажей (#115): список найденных персонажей (аватар+пол+голос+реплики) / сохранение правок.
  casting: (pid: string) => getJson<{ characters: Character[] }>(`/projects/${pid}/casting`),
  castingAvatarUrl: (pid: string, id: string) => `${BASE}/projects/${pid}/casting/avatar?id=${encodeURIComponent(id)}`,
  castingVoiceUrl: (pid: string, id: string) => `${BASE}/projects/${pid}/casting/voice?id=${encodeURIComponent(id)}`,   // wav образца голоса персонажа (<audio>/new Audio)
  setCasting: (pid: string, characters: { id: string; name: string; speech_note: string; dub_voice: string | null }[]) =>
    postJson<{ ok: boolean; characters: Character[] }>(`/projects/${pid}/casting`, { characters }),
  // Библиотека кастингов (#115): сохранить текущий кастинг проекта как именованный профиль и применить его
  // к другому ролику через analyze(..., casting_ref=<slug>). Профили переживают проекты (общая база актёров).
  castingLibrary: () => getJson<{ casts: { slug: string; name: string; char_count: number }[] }>("/casting/library"),
  saveCastingToLibrary: (pid: string, name: string) => postJson<{ slug: string }>(`/projects/${pid}/casting/library`, { name }),
  deleteCastingLibrary: (slug: string) => fetch(`${BASE}/casting/library/${encodeURIComponent(slug)}`, { method: "DELETE" }).then(j<{ ok: boolean }>),
  castingLibraryAvatarUrl: (slug: string, id: string) => `${BASE}/casting/library/${encodeURIComponent(slug)}/avatar?id=${encodeURIComponent(id)}`,
  // Глоссарий проекта и профиля сериала (библиотека кастингов); «Собрать из текста» — джоба с кандидатами в итоге.
  glossary: (pid: string) => getJson<ProjectGlossary>(`/projects/${pid}/glossary`),
  glossaryTsv: (pid: string) => fetch(`${BASE}/projects/${pid}/glossary?format=tsv`).then(async (r) => { if (!r.ok) throw new Error(`${r.status} ${await r.text()}`); return r.text(); }),
  saveGlossary: (pid: string, body: GlossaryPut) => sendCoded<ProjectGlossary>("PUT", `/projects/${pid}/glossary`, body),
  extractGlossary: (pid: string) => postJson<{ job_id: string }>(`/projects/${pid}/glossary/extract`, {}),
  seriesGlossary: (slug: string) => getJson<{ slug: string; entries: GlossaryEntry[] }>(`/casting/library/${encodeURIComponent(slug)}/glossary`),
  saveSeriesGlossary: (slug: string, body: GlossaryPut) =>
    sendCoded<{ slug: string; entries: GlossaryEntry[] }>("PUT", `/casting/library/${encodeURIComponent(slug)}/glossary`, body),
  listProjects: () => getJson<{ projects: ProjectListing[] }>("/projects"),   // недавние/сохранённые проекты для экрана «Открыть»
  getProject: (pid: string) => getJson<Project>(`/projects/${pid}`),
  deleteProject: (pid: string) => fetch(`${BASE}/projects/${pid}`, { method: "DELETE" }).then(j<{ ok: boolean }>),   // удалить проект (стирает workspace/<pid>) — кнопка в «Недавних»
  // undo/redo: та же очередь, что у patch(). base — ревизия, на которой снят снимок (без неё — последняя известная
  // окну). Если после неё проект сохранял кто-то, кроме этого окна, сервер его не перезапишет: ApiError project_changed.
  putProject: (pid: string, project: Project, base?: number) =>
    _chain(() => fetch(`${BASE}/projects/${pid}`, {
      method: "PUT", headers: base === undefined ? JSON_HEADERS : { ...JSON_HEADERS, [REV_HEADER]: String(base) }, body: JSON.stringify(project),
    }).then(coded<Project>)),
  patch: (pid: string, edit: Record<string, unknown>) =>   // run after the previous patch settles (ok or failed)
    _chain(() => fetch(`${BASE}/projects/${pid}`, { method: "PATCH", headers: JSON_HEADERS, body: JSON.stringify(edit) }).then(j<Project>)),
  alignProject: (pid: string) => fetch(`${BASE}/projects/${pid}/align`, { method: "POST" }).then(j<{ job_id: string; project_id: string }>),
  render: (pid: string) => fetch(`${BASE}/projects/${pid}/render`, { method: "POST" }).then(j<{ job_id: string }>),
  // Экспорт-уровень мультиязыка: клон отредактированного проекта на язык lang (наследует раскладку/стиль/
  // блюр/титры + клон голоса), ре-перевод текста + рендер одним джобом. -> новый project_id + job_id.
  exportLang: (pid: string, lang: string) => fetch(`${BASE}/projects/${pid}/export-lang?lang=${encodeURIComponent(lang)}`, { method: "POST" }).then(j<{ job_id: string; project_id: string }>),
  // #122: смена режима из транскрипта — перевод готовых сегментов на lang + смена режима, БЕЗ повторного ASR.
  retranslate: (pid: string, lang: string, mode: string) => fetch(`${BASE}/projects/${pid}/retranslate?lang=${encodeURIComponent(lang)}&mode=${encodeURIComponent(mode)}`, { method: "POST" }).then(j<{ job_id: string; project_id: string }>),
  dubAudio: (pid: string) => fetch(`${BASE}/projects/${pid}/dub-audio`, { method: "POST" }).then(j<{ job_id: string }>),   // сгенерить только озвучку (без сборки видео) — слушать дуб в редакторе
  // Сократить перевод реплик под слот (джоба shorten): названные строки или все, что не влезают.
  shorten: (pid: string, target: { ids: string[] } | { all_over: true }) => postJson<{ job_id: string }>(`/projects/${pid}/shorten`, target),
  takes: (pid: string, id: string) => getJson<Takes>(`/projects/${pid}/segments/${encodeURIComponent(id)}/takes`),
  takeAudioUrl: (pid: string, id: string, n: number) => `${BASE}/projects/${pid}/segments/${encodeURIComponent(id)}/takes/${n}/audio`,
  remix: (pid: string, instruction: string) =>
    fetch(`${BASE}/projects/${pid}/remix?instruction=${encodeURIComponent(instruction)}`, { method: "POST" }).then(j<{ job_id: string }>),
  previewUrl: (pid: string, t: number, rev = 0, lowres = false) => `${BASE}/projects/${pid}/preview?t=${t}&rev=${rev}${lowres ? "&lr=1" : ""}`,   // lr=1 при плее -> низкое разрешение на больших видео (быстрее)
  originalUrl: (pid: string, t: number) => `${BASE}/projects/${pid}/original?t=${t}`,
  waveform: (pid: string) => getJson<{ peaks: number[] }>(`/projects/${pid}/waveform`),
  outputUrl: (pid: string) => `${BASE}/projects/${pid}/output`,
  openOutput: (pid: string) => fetch(`${BASE}/projects/${pid}/open`, { method: "POST" }).then(j<{ ok: boolean }>),   // открыть output.mp4 в системном плеере (нативный webview не открывает target=_blank)
  reveal: (pid: string, name: string) => postJson<{ ok: boolean }>(`/projects/${pid}/reveal`, { name }),   // показать файл в проводнике с выделением
  saveText: (pid: string, name: string, text: string) => postJson<{ ok: boolean; path: string }>(`/projects/${pid}/save-text`, { name, text }),   // записать SRT/TXT в каталог проекта + reveal (webview не качает blob)
  pickFolder: () => postJson<{ dir: string | null }>("/pick-folder", {}),   // нативный диалог выбора папки (batch-экспорт в одну папку)
  saveOutput: (pid: string, dir: string, name: string) => postJson<{ ok: boolean; path?: string }>(`/projects/${pid}/save-output`, { dir, name }),   // копия готового output в dir под именем оригинала
  dubUrl: (pid: string, rev = 0) => `${BASE}/projects/${pid}/dub?rev=${rev}`,   // playable dubbed video (frames + dub audio)
  // Джобы: активные и недавние по проекту (+ его job.json), снапшот, отмена, продолжение с места остановки.
  jobs: (pid: string) => getJson<{ jobs: JobSnapshot[]; project_job: ProjectJob | null }>(`/jobs?pid=${encodeURIComponent(pid)}`),
  job: (jobId: string) => getJson<JobSnapshot>(`/jobs/${jobId}`),
  // Long-poll: снапшот при завершении джобы или через `secs` (≤55); null — джобы уже нет в истории.
  waitJob: (jobId: string, secs: number) => fetch(`${BASE}/jobs/${jobId}?wait=${secs}`).then((r) => (r.status === 404 ? null : j<JobSnapshot>(r))),
  cancelJob: (jobId: string) => fetch(`${BASE}/jobs/${jobId}/cancel`, { method: "POST" }).then(j<{ id: string; status: string }>),
  resumeProject: (pid: string) => fetch(`${BASE}/projects/${pid}/resume`, { method: "POST" }).then(j<{ job_id: string; kind: string; project_id: string }>),
  // MCP-сервер студии: адрес для подключения агента и его статус для раздела настроек «Агент (MCP)».
  mcpUrl: () => `${BASE || window.location.origin}/mcp`,
  mcpStatus: () => getJson<McpStatus>("/mcp/status"),
  // SSE job progress -> onEvent per message; resolves on done, rejects on error
  // signal: the watcher went away (unmount, project switch) — the stream closes, the promise rejects with AbortError.
  watchJob: (jobId: string, onEvent: (e: JobEvent) => void, signal?: AbortSignal) =>
    new Promise<unknown>((resolve, reject) => {
      if (signal?.aborted) { reject(new DOMException("aborted", "AbortError")); return; }
      const es = new EventSource(`${BASE}/jobs/${jobId}/events`);
      signal?.addEventListener("abort", () => { es.close(); reject(new DOMException("aborted", "AbortError")); }, { once: true });
      es.onmessage = (m) => {
        try {
          const e: JobEvent = JSON.parse(m.data);
          onEvent(e);                                  // a consumer throw must not leak the stream open either
          if (e.type === "done") { es.close(); resolve(e.result); }
          else if (e.type === "error") { es.close(); reject(new Error(e.error)); }
          else if (e.type === "cancelled") { es.close(); reject(new JobCancelledError()); }
        } catch (err) { es.close(); reject(err instanceof Error ? err : new Error(String(err))); }
      };
      // EventSource fires onerror on transient drops too (it auto-reconnects) — only give up once truly CLOSED
      es.onerror = () => { if (es.readyState === EventSource.CLOSED) reject(new Error(i18n.t("common.streamLost"))); };
    }),
};

// created — рождение каталога проекта (секунды эпохи); null, если ФС его не хранит.
export type ProjectListing = ProjectSummary & { created: number | null };
export type LaunchDefaults = {
  audio: "nodub" | "dub" | "voiceover" | "transcribe";
  subs: "none" | "transcribe" | "translate" | "bilingual";
  burn: boolean; detect_text: boolean;
  src_lang: string; tgt_lang: string | null;   // tgt_lang null — язык интерфейса окна
  speaker_count: number;
  casting: boolean; casting_ref: string; content_type: "auto" | "real" | "anime";
  vo_gain_db: number;
  tr_style: "" | "technical" | "literary" | "casual" | "custom"; tr_style_custom: string;
  sub_blur: boolean; keep_orig: boolean; container: "mp4" | "mkv";
  voice_src: "clone" | "library"; voice_slots_m: string[]; voice_slots_f: string[];
};
export type LaunchDefaultsState = { defaults: LaunchDefaults; saved: boolean };
export type AppPaths = { data_dir: string; projects_dir: string; models_dir: string };
