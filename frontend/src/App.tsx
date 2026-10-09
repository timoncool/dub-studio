import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { FolderOpen } from "lucide-react";
import { DONATE } from "./lib/links";
import { OPEN_SETTINGS_EVENT, SETTINGS_SECTIONS, openSettings } from "./lib/settingsNav";
import { createLaunchSaver, loadWithMigration } from "./lib/launchDefaults";
import SettingsModal from "./components/settings/SettingsModal";
import ProjectsList, { MODE_KEYS } from "./components/ProjectsList";
import ConfirmDialog from "./components/ConfirmDialog";
import ServerOffline from "./components/ServerOffline";
import { motion } from "motion/react";
import { Upload, Languages, AudioLines, Sparkles, ArrowRight, ShieldCheck, Download, Loader2, Trash2, Plus, Captions, Columns2, FolderDown, ExternalLink, X, Undo2, Redo2, Settings, Eye, EyeOff, Play, Pause, RotateCw, RefreshCw, Square, Droplet, Check, HelpCircle, Copy, Star, Music, Move, Minimize2, FileText, Users, Mic2, AlignLeft, AlignCenter, AlignRight, ChevronFirst, ChevronLast, ArrowLeftToLine, ArrowRightToLine, ChevronDown, ChevronUp, GripVertical, ScrollText, Clock, Keyboard, Save, ZoomIn, ZoomOut, Sliders, Merge, Scissors, Bot, History, Pin } from "lucide-react";
import { createPortal } from "react-dom";
import { useFloatable, dockSlot } from "./lib/useFloatable";
import { api, llmProviderOf, slot, ApiError, SetupError, JobCancelledError, type Selection, type AnalyzePost, type AnalyzeResult, type JobKind, type Project, type Capabilities, type SetupStatus, type SetupComponent, type ProjectListing, type LaunchDefaults, type Character, type DownloadJob, type UrlFetch, type ShortenResult } from "./lib/api";
import { LANGS, DUB_LANGS, setLang, type Lang } from "./lib/i18n";
import { useStore } from "./store";
import PreviewCanvas from "./components/PreviewCanvas";
import SubsContentControl from "./components/SubsContentControl";
import { subtitleText } from "./lib/subtitleText";
import { playSfx } from "./lib/sfx";
import ResourceMonitor from "./components/ResourceMonitor";
import { GENDER_LABEL_KEY, genderKey } from "./lib/gender";
import OpenRouterKey from "./components/OpenRouterKey";
import GoogleTts, { GoogleKey } from "./components/GoogleTts";
import ProxySection from "./components/ProxySection";
import { ANALYZE_STEPS, STAGE_TO_STEPKEY } from "./lib/stages";
import { enqueueWhenFree, finishAnalyze, reportVoiceSlots, watchLocal, watchTracked, watchWithResume } from "./lib/jobs";
import CancelJobButton from "./components/CancelJobButton";
import JobFailurePanel from "./components/JobFailurePanel";
import ProjectJobBar from "./components/ProjectJobBar";
import { ContinueJobButton, JobStateLabel } from "./components/RecentJobBadge";
import AgentPanel from "./components/AgentPanel";
import WhatsNew from "./components/WhatsNew";
import DownloadProgress from "./components/DownloadProgress";
import ModelsFolder from "./components/ModelsFolder";
import GpuNotice from "./components/GpuNotice";
import RemoveComponent from "./components/RemoveComponent";
import { useSetupStatus } from "./lib/useSetupStatus";
import { useDownloadErrorText, useGpuReasonText } from "./lib/setupText";
import { fmtBytes } from "./lib/format";
import { makeSpeakerVoice } from "./lib/speakerVoice";
import SubsAlignToggle from "./components/SubsAlignToggle";
import UrlImport from "./components/UrlImport";
import YtDlpTool from "./components/YtDlpTool";
import FitBadge from "./components/FitBadge";
import FitToolbar from "./components/FitToolbar";
import TakesPanel from "./components/TakesPanel";
import { fitOver } from "./lib/fit";
import LlmProviders from "./components/LlmProviders";
import OpenRouterModelSelect, { OpenRouterCatalogRow } from "./components/OpenRouterModelSelect";
import BridgeHost from "./components/BridgeHost";
import { useChanged, useEditorBridge, useProjectSync, useTranscriptBridge, type Draft } from "./components/editorBridge";
import { CASTING_CHANGED, PROJECTS_CHANGED, SETTINGS_CHANGED, VOICES_CHANGED, bridgeError, takeArgs, textArg, useBridgeCommand } from "./lib/mcpBridge";
import { named } from "./lib/a11y";
import { goHome, openProject as openProjectIn } from "./lib/openProject";
import { GlossaryButton } from "./components/GlossaryPanel";
import { SourceText, TtsSkipNote } from "./components/SegmentText";
import { hubChanged, setTelemetry, useHubState } from "./lib/studioHub";
import HubTelemetryPreview from "./components/HubTelemetryPreview";

const EASE = [0.22, 1, 0.36, 1] as const;

// timecode m:ss.d — the navigation reference shown on every segment + the scrub readout
function fmtT(s: number) {
  const m = Math.floor(s / 60), sec = Math.floor(s % 60), d = Math.floor((s * 10) % 10);
  return `${m}:${String(sec).padStart(2, "0")}.${d}`;
}

// Имя файла из пути (Windows/POSIX-разделители); fallback — сам путь.
const baseName = (path: string) => path.split(/[\\/]/).pop() || path;

// Начальный playhead: ?t=SEC из URL (deep-link на кадр — для скринов/шаринга), иначе 1.0с.
const initialScrub = (): number => {
  try {
    const q = new URLSearchParams(location.search).get("t");
    const n = q ? parseFloat(q) : NaN;
    if (Number.isFinite(n) && n >= 0) return n;
  } catch { /* нет URL/DOM */ }
  return 1.0;
};

// Направления тени титра/субтитра (значение, стрелка) — единый список для двух селектов.
const SHADOW_DIRS: [string, string][] = [
  ["", "—"], ["270", "↑"], ["315", "↗"], ["0", "→"], ["45", "↘"],
  ["90", "↓"], ["135", "↙"], ["180", "←"], ["225", "↖"],
];

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

// Инверсия backend component_selection: id варианта -> (слот active.json, значение). Нужна, чтобы
// показать в дропдауне РЕАЛЬНО активный вариант (а не «первый установленный»), совпадая с тем, что
// резолвится при генерации.
const VARIANT_SLOT: Record<string, [string, string]> = {
  higgs: ["tts", "q8_0"], "higgs-q6_k": ["tts", "q6_k"], "higgs-q4_k_m": ["tts", "q4_k_m"],
  parakeet: ["asr", "int8"], "parakeet-fp32": ["asr", "fp32"], "parakeet-ultra": ["asr", "ultra"],
  "parakeet-ultra-int8": ["asr", "ultra-int8"],
  gemma: ["mt", "q4_0"], "gemma-q5_0": ["mt", "q5_0"], "gemma-q6_k": ["mt", "q6_k"], "gemma-q8_0": ["mt", "q8_0"],
  roformer: ["sep", "Q8_0"], "roformer-q5": ["sep", "Q5_0"], "roformer-q4": ["sep", "Q4_0"],
  "whisper-tiny": ["whisper_model", "tiny"], "whisper-base": ["whisper_model", "base"],
  "whisper-small": ["whisper_model", "small"], "whisper-medium": ["whisper_model", "medium"],
  "whisper-large-v3": ["whisper_model", "large-v3"], "whisper-large-v3-turbo": ["whisper_model", "large-v3-turbo"],
};
// Какой из ids сейчас активен по выбору (active.json из capabilities.selection).
const activeVariantId = (ids: string[], sel: Selection): string | undefined =>
  ids.find((id) => { const m = VARIANT_SLOT[id]; return !!m && sel[m[0]] === m[1]; });

type VariantI18n = Record<string, { label: "asrVariant.int8" | "asrVariant.fp32" | "asrVariant.ultra" | "asrVariant.ultraInt8"; hint: "asrVariant.int8Hint" | "asrVariant.fp32Hint" | "asrVariant.ultraHint" | "asrVariant.ultraInt8Hint" }>;
// Варианты Parakeet: int8 и fp32 — базовая модель NVIDIA, Ultra — её дообученная Moondream версия.
const ASR_VARIANT_I18N: VariantI18n = {
  parakeet: { label: "asrVariant.int8", hint: "asrVariant.int8Hint" },
  "parakeet-fp32": { label: "asrVariant.fp32", hint: "asrVariant.fp32Hint" },
  "parakeet-ultra": { label: "asrVariant.ultra", hint: "asrVariant.ultraHint" },
  "parakeet-ultra-int8": { label: "asrVariant.ultraInt8", hint: "asrVariant.ultraInt8Hint" },
};

// 25 европейских языков, которые распознаёт дефолтный ASR Parakeet-TDT v3. Источник вне этого набора
// требует Whisper (99 языков) — переключаем движок автоматически с уведомлением.
const PARAKEET_LANGS = new Set([
  "en", "es", "fr", "ru", "de", "it", "pl", "uk", "ro", "nl", "hu", "el", "sv",
  "cs", "bg", "pt", "sk", "hr", "da", "fi", "lt", "sl", "lv", "et", "mt",
]);

// Модели и компоненты в настройках: список из /setup/status с кнопками скачки/докачки и прогрессом.
// part="cloud" — раздел «Облако» (ключ OpenRouter и потоки) на том же состоянии выбора.
function ModelsSection({ part = "models" }: { part?: "models" | "cloud" }) {
  const { t } = useTranslation();
  const errText = useDownloadErrorText();
  const gpuReasonText = useGpuReasonText();
  // Выбор ASR-движка (parakeet|whisper) + квант Whisper (compute) — из capabilities.selection.
  const [cap, setCap] = useState<Capabilities | null>(null);
  const [asrEngine, setAsrEngine] = useState<string>("parakeet");
  const [whisperCompute, setWhisperCompute] = useState<string>("int8");
  // Выбор варианта в дропдаунах поднят СЮДА (а не в локальный useState VariantPicker) — иначе ре-рендер
  // секции ремаунтил пикер и сбрасывал выбор на дефолт. Ключ = ids[0] группы.
  const [picks, setPicks] = useState<Record<string, string>>({});
  const [err, setErr] = useState<string | null>(null);            // ошибки скачки/импорта — показываем, не глотаем
  // Облачные движки OpenRouter — НЕ отдельный блок, а альтернатива локальному движку ВНУТРИ каждой группы
  // (перевод: Gemma|свой сервер|OpenRouter, TTS: Higgs|OpenRouter), как Parakeet|Whisper в ASR. Ключ общий на все стадии.
  const [orVoices, setOrVoices] = useState<{ name: string; gender: string; age: string; ru: boolean }[]>([]);
  const [ttsRu, setTtsRu] = useState<boolean | null>(null);
  const loadCap = () => api.capabilities().then((c) => {
    setCap(c);
    setAsrEngine(slot(c.selection, "asr_engine") ?? "parakeet");
    setWhisperCompute(slot(c.selection, "whisper_compute") ?? "int8");
  }).catch(() => {});
  // Статус компонентов + фоновая закачка (опрос, пока она идёт). Скачанный вариант становится активным на
  // бэке — после закачки перечитываем выбор.
  const { status, error: statusErr, refresh: refreshStatus, adopt, downloading } = useSetupStatus(() => { loadCap(); });
  const prog = downloading ? status?.active ?? null : null;
  const partPct = (id: string) => {
    const p = prog?.parts.find((x) => x.id === id);
    return p && p.total > 0 ? (p.done / p.total) * 100 : prog?.ids.includes(id) ? 0 : null;
  };
  useEffect(() => { loadCap(); }, []);
  const selv = (k: string) => slot(cap?.selection, k) ?? "";
  const hasOrKey = cap?.selection?.or_key_set === true;
  const setSel = (k: string, v: string) => api.setSelection(k, v).then(loadCap).catch(() => {});
  // Голоса выбранной облачной TTS-модели (пол/возраст/русский) для дропдауна + предупреждения о русском.
  const ttsProvider = slot(cap?.selection, "tts_provider") ?? (selv("or_tts_on") === "1" ? "openrouter" : "local");
  const orTtsModel = slot(cap?.selection, ttsProvider === "google" ? "google_tts_model" : "or_tts_model") ?? "";
  useEffect(() => {
    let active = true;
    const voices = orTtsModel ? api.openrouterVoices(orTtsModel) : Promise.resolve({voices:[],supportsRussian:null});
    voices.then((r) => { if (active) { setOrVoices(r.voices); setTtsRu(r.supportsRussian); } }).catch((e:unknown)=>{ if (active) setErr(e instanceof Error ? e.message : String(e)); });
    return () => { active = false; };
  }, [orTtsModel]);
  const setupErr = (e: unknown) => setErr(e instanceof SetupError ? `${errText(e.code)} · ${e.detail}` : e instanceof Error ? e.message : String(e));
  const dl = async (ids: string[]) => {
    if (downloading) return;
    setErr(null);
    try { await api.setupDownload(ids); await refreshStatus(); } catch (e) { setupErr(e); }
  };
  const pause = () => { api.setupCancel().then(() => refreshStatus()).catch(setupErr); };
  useChanged(SETTINGS_CHANGED, () => { loadCap(); refreshStatus().catch(setupErr); });
  const browseId = async (id: string) => {
    if (prog) return;
    try {
      const r = await api.setupBrowse(id);
      if (r.picked) adopt(r.status);
      if (r.errors.length > 0) setErr(`${t("downloads.importErrors")} · ${r.errors.join("; ")}`);
    } catch (e) { setupErr(e); }
  };
  const BrowseBtn = ({ id }: { id: string }) => (
    <button onClick={() => browseId(id)} disabled={!!prog} title={t("settings.browseFolder")}
      className="shrink-0 inline-flex items-center px-1.5 py-1 rounded-md border border-[var(--color-border)] text-[var(--color-muted)] hover:border-[var(--color-accent)] hover:text-[var(--color-text)] disabled:opacity-40">
      <FolderDown size={12} />
    </button>
  );
  if (part === "cloud") return (
    <div className="max-w-2xl space-y-2">
      <div data-settings-part="key"><OpenRouterKey onSaved={loadCap} /></div>
      <GoogleKey onSaved={loadCap} />
      {hasOrKey && <OpenRouterCatalogRow />}
      {hasOrKey && (
        <div className="px-2.5 py-2 rounded-lg bg-[var(--color-surface-2)] border border-[var(--color-border)]">
          <div className="flex items-center gap-2.5">
            <span className="w-1.5 h-1.5 rounded-full shrink-0 bg-[var(--color-muted)]" />
            <div className="min-w-0 flex-1">
              <div className="text-[12px] font-medium truncate">{t("cloud.threads")}</div>
              <div className="mono text-[10px] text-[var(--color-muted)] truncate">{t("cloud.threadsHint")}</div>
            </div>
            <select value={selv("or_concurrency") || "6"} onChange={(e) => setSel("or_concurrency", e.target.value)}
              className="shrink-0 bg-[var(--color-surface)] border border-[var(--color-border)] rounded-md px-2 py-1 text-[11px] mono focus:border-[var(--color-accent)] focus:outline-none">
              {["1", "2", "4", "6", "8", "12", "16"].map((o) => <option key={o} value={o}>{o}</option>)}
            </select>
          </div>
        </div>
      )}
    </div>
  );

  if (!status) return <div className="mono text-[11px] text-[var(--color-muted)]">{statusErr ?? "…"}</div>;
  const byId = Object.fromEntries(status.components.map((c) => [c.id, c]));
  const get = (id: string) => byId[id] as SetupComponent | undefined;

  // строка одного компонента (кнопка скачать/докачать + прогресс)
  const Row = (c: SetupComponent) => {
    const pct = partPct(c.id);
    const active = pct != null;
    return (
      <div key={c.id} className="flex items-center gap-2.5 px-2.5 py-2 rounded-lg bg-[var(--color-surface-2)] border border-[var(--color-border)]">
        <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${c.installed ? "bg-[var(--color-accent)]" : "bg-[var(--color-muted)]"}`} />
        <div className="min-w-0 flex-1">
          <div className="text-[12px] font-medium truncate">{c.name}</div>
          <div className="mono text-[10px] text-[var(--color-muted)] truncate">{c.purpose} · {c.vram ? `${fmtBytes(c.vram)} VRAM · ` : ""}{fmtBytes(c.size)} {t("settings.disk")}</div>
        </div>
        {active ? (
          <div className="w-24 shrink-0">
            <div className="h-1.5 rounded-full bg-white/10 overflow-hidden"><div className="h-full bg-[var(--color-accent)] transition-[width]" style={{ width: `${pct ?? 0}%` }} /></div>
            <div className="mono text-[10px] text-[var(--color-muted)] text-right mt-0.5">{Math.round(pct ?? 0)}%</div>
          </div>
        ) : (
          <div className="flex items-center gap-1.5 shrink-0">
            <RemoveComponent c={c} disabled={!!prog} onDone={(s) => { adopt(s); loadCap(); }} onError={setErr} />
            <BrowseBtn id={c.id} />
            <button onClick={() => dl([c.id])} disabled={!!prog}
              className="inline-flex items-center gap-1 px-2.5 py-1 rounded-md text-[12px] border border-[var(--color-border)] text-[var(--color-accent-2)] hover:border-[var(--color-accent)] disabled:opacity-40">
              {c.installed ? <RefreshCw size={12} /> : <Download size={12} />}{c.installed ? t("settings.redownload") : t("setup.downloadOne")}
            </button>
          </div>
        )}
      </div>
    );
  };

  // модель с выбором кванта: дропдаун вариантов + скачать выбранный. КОНТРОЛИРУЕМЫЙ — выбранное значение
  // берётся из поднятого picks / активного выбора (active.json), НЕ из внутреннего useState (иначе ре-рендер
  // секции сбрасывал бы дропдаун на дефолт). При смене — пишем picks и активируем на бэке (если установлен).
  const VariantPicker = ({ base, ids, i18n }: { base: string; ids: string[]; i18n?: VariantI18n }) => {
    const variants = ids.map(get).filter(Boolean) as SetupComponent[];
    const installed = variants.find((v) => v.installed);
    const sel = cap?.selection ?? {};
    const pick = picks[ids[0]] ?? activeVariantId(ids, sel) ?? installed?.id ?? variants[0]?.id ?? "";
    const c = get(pick);
    if (!c) return null;
    const quant = (v: SetupComponent) => i18n?.[v.id] ? t(i18n[v.id].label) : (v.name.match(/\b(q\d[\w]*|int8|fp32|f16|large-v3-turbo|large-v3|tiny|base|small|medium)\b/i)?.[1] ?? v.name);
    const hint = i18n?.[c.id] ? t(i18n[c.id].hint) : "";
    const pct = partPct(c.id);
    const active = pct != null;
    return (
      <div className="px-2.5 py-2 rounded-lg bg-[var(--color-surface-2)] border border-[var(--color-border)]">
        <div className="flex items-center gap-2.5">
          <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${variants.some((v) => v.installed) ? "bg-[var(--color-accent)]" : "bg-[var(--color-muted)]"}`} />
          <div className="min-w-0 flex-1">
            <div className="text-[12px] font-medium truncate">{base}</div>
            {hint && <div className="text-[10px] leading-snug text-[var(--color-muted)]">{hint}</div>}
            <div className="mono text-[10px] text-[var(--color-muted)] truncate">{c.vram ? `${fmtBytes(c.vram)} VRAM · ` : ""}{fmtBytes(c.size)} {t("settings.disk")}{c.installed ? "" : ` · ${t("settings.notInstalled")}`}</div>
          </div>
          <select value={pick} onChange={(e) => {
              const id = e.target.value;
              setPicks((p) => ({ ...p, [ids[0]]: id }));
              const sv = get(id);
              // установлен -> активируем сразу; не установлен -> активируется автоматически после скачки
              if (sv?.installed) api.selectModel(id).then(loadCap).catch((er) => setErr(er instanceof Error ? er.message : String(er)));
            }}
            className="shrink-0 bg-[var(--color-surface)] border border-[var(--color-border)] rounded-md px-2 py-1 text-[11px] mono focus:border-[var(--color-accent)] focus:outline-none">
            {variants.map((v) => <option key={v.id} value={v.id}>{quant(v)}{v.installed ? " ✓" : ""} · {fmtBytes(v.size)}{v.vram ? ` / ${fmtBytes(v.vram)} VRAM` : ""}</option>)}
          </select>
          {active ? (
            <span className="mono text-[11px] text-[var(--color-accent)] w-10 text-right">{Math.round(pct ?? 0)}%</span>
          ) : (
            <>
              <RemoveComponent c={c} disabled={!!prog} onDone={(s) => { adopt(s); loadCap(); }} onError={setErr} />
              <BrowseBtn id={c.id} />
              <button onClick={() => dl([c.id])} disabled={!!prog} title={c.installed ? t("settings.redownload") : t("setup.downloadOne")}
                className="shrink-0 inline-flex items-center gap-1 px-2 py-1 rounded-md text-[12px] border border-[var(--color-border)] text-[var(--color-accent-2)] hover:border-[var(--color-accent)] disabled:opacity-40">
                {c.installed ? <RefreshCw size={12} /> : <Download size={12} />}
              </button>
            </>
          )}
        </div>
      </div>
    );
  };

  const Group = ({ label, children }: { label: string; children: React.ReactNode }) => (
    <div className="mb-3">
      <div className="text-[11px] uppercase tracking-[0.14em] text-[var(--color-muted)] mb-1.5">{label}</div>
      <div className="space-y-1.5">{children}</div>
    </div>
  );

  const rowOf = (id: string) => { const c = get(id); return c ? <Row key={id} {...c} /> : null; };

  // Тумблер движка «локально | OpenRouter» — тот же вид, что переключатель Parakeet|Whisper в группе ASR.
  const orRowCls = "px-2.5 py-2 rounded-lg bg-[var(--color-surface-2)] border border-[var(--color-border)]";
  // На чём считать стадию (устройство): Авто / GPU (CUDA) / CPU — свои табы в каждом разделе, по
  // аналогии с провайдерами. Независимый ключ на стадию (sep_backend / diar_backend / asr_backend).
  const BackendTabs = ({ k }: { k: string }) => {
    const cur = selv(k) || "auto";
    const gpuOff = !status.gpu.cuda13Ok;
    return (
      <div className="flex gap-1 mb-1.5">
        {([["auto", t("settings.auto")], ["gpu", "GPU (CUDA)"], ["cpu", "CPU"]] as const).map(([id, label]) => (
          <button key={id} onClick={() => setSel(k, id)} disabled={id === "gpu" && gpuOff && cur !== "gpu"}
            title={id === "gpu" && gpuOff ? gpuReasonText(status.gpu) : undefined}
            className={`flex-1 px-2 py-1.5 rounded-md text-[12px] font-medium border transition-colors ${cur === id ? "border-[var(--color-accent)] bg-[color-mix(in_oklab,var(--color-accent)_14%,transparent)] text-[var(--color-text)]" : "border-[var(--color-border)] text-[var(--color-muted)] hover:text-[var(--color-text)]"} disabled:opacity-40`}>{label}</button>
        ))}
      </div>
    );
  };
  const orSelectCls = "w-full bg-[var(--color-surface)] border border-[var(--color-border)] rounded-md px-2 py-1 text-[11px] mono focus:border-[var(--color-accent)] focus:outline-none";
  // Модели OpenRouter стадии: имя, цена, контекст — из каталога (кэш на сервере).
  const orModelSelect = (kind: "tts" | "asr", k: string, empty: string) => (
    <OpenRouterModelSelect kind={kind} value={selv(k)} onChange={(id) => { if (id) setSel(k, id); }} placeholder={empty} />
  );
  const needsGemma = llmProviderOf(cap?.selection, "llm") === "local" || llmProviderOf(cap?.selection, "vision") === "local";

  const browse = async () => {
    if (prog) return;
    try {
      const r = await api.setupBrowse();
      if (r.picked) adopt(r.status);
      if (r.errors.length > 0) setErr(`${t("downloads.importErrors")} · ${r.errors.join("; ")}`);
    } catch (e) { setupErr(e); }
  };
  const driverUrl = get("nvidia-driver")?.externalUrl;

  return (
    <div>
      <div className="mb-3 space-y-2">
        <ModelsFolder status={status} />
        <GpuNotice gpu={status.gpu} driverUrl={driverUrl} />
        {status.active && status.active.status !== "completed" && (
          <DownloadProgress job={status.active} onPause={pause} onResume={(ids) => { dl(ids); }} />
        )}
      </div>
      <button onClick={browse} disabled={!!prog}
        className="mb-3 w-full inline-flex items-center justify-center gap-2 px-3 py-2 rounded-lg border border-dashed border-[var(--color-border)] text-[12px] text-[var(--color-muted)] hover:border-[var(--color-accent)] hover:text-[var(--color-text)] transition-colors disabled:opacity-40">
        <FolderDown size={14} />{t("settings.browseFolder")}
      </button>
      {err && (
        <div className="mb-3 flex items-start gap-2 px-2.5 py-2 rounded-lg border border-[var(--color-danger,#ef4444)]/40 bg-[color-mix(in_oklab,#ef4444_10%,transparent)] text-[11px]">
          <span className="mono text-[var(--color-danger,#ef4444)] break-words flex-1">{err}</span>
          <button onClick={() => setErr(null)} className="shrink-0 text-[var(--color-muted)] hover:text-[var(--color-text)]"><X size={12} /></button>
        </div>
      )}
      <Group label={t("settings.roleTts")}>
        <div className="flex gap-1 mb-2">
          {(["local", "openrouter", "google"] as const).map(provider => <button key={provider} onClick={() => api.setSelection("tts_provider",provider).then(loadCap).catch(e=>setErr(String(e)))}
            className={`flex-1 px-2 py-1.5 rounded-md border text-[12px] ${ttsProvider === provider ? "border-[var(--color-accent)]" : "border-[var(--color-border)]"}`}>{t(`google.provider_${provider}`)}</button>)}
        </div>
        {ttsProvider !== "local" ? (
          <div className={`${orRowCls} space-y-2`}>
            {ttsProvider === "google" ? <GoogleTts model={selv("google_tts_model")} mode={selv("google_tts_mode")} onChange={(key,value)=>{ api.setSelection(key,value).then(loadCap).catch(e=>setErr(String(e))); }} /> : orModelSelect("tts", "or_tts_model", t("providers.pickTtsModel"))}
            {ttsRu === false && <div className="text-[11px] text-[var(--color-warn)]">{t("cloud.ttsNoRussian")}</div>}
            <div className="flex items-center gap-2">
              <button onClick={() => setSel("or_tts_autocast", (selv("or_tts_autocast") || "1") !== "0" ? "0" : "1")}
                role="switch" aria-checked={(selv("or_tts_autocast") || "1") !== "0"} className={`relative w-9 h-5 rounded-full transition-colors shrink-0 ${(selv("or_tts_autocast") || "1") !== "0" ? "bg-[var(--color-accent)]" : "bg-[var(--color-surface)] border border-[var(--color-border)]"}`}>
                <span className={`absolute top-0.5 w-4 h-4 rounded-full bg-white transition-all ${(selv("or_tts_autocast") || "1") !== "0" ? "left-[18px]" : "left-0.5"}`} />
              </button>
              <span className="text-[12px]">{t("cloud.autocast")}</span>
              <span className="text-[9px] px-1.5 py-0.5 rounded bg-[var(--color-accent)]/20 text-[var(--color-accent)] uppercase tracking-wider">{t("cloud.beta")}</span>
            </div>
            {(selv("or_tts_autocast") || "1") !== "0" ? (
              <div className="text-[11px] text-[var(--color-muted)]">{t("cloud.autocastHint")}</div>
            ) : (
              <select value={selv("or_tts_voice")} onChange={(e) => setSel("or_tts_voice", e.target.value)} className={orSelectCls}>
                <option value="">{t("cloud.oneVoice")}</option>
                {orVoices.map((v) => <option key={v.name} value={v.name}>{v.gender === "male" ? "♂" : v.gender === "female" ? "♀" : "•"} {v.name}{v.age === "teen" ? ` · ${t("voice.ageTeen")}` : v.age === "child" ? ` · ${t("voice.ageChild")}` : ""}</option>)}
              </select>
            )}
          </div>
        ) : (
          <>
            <VariantPicker base="Higgs Audio v3" ids={["higgs", "higgs-q6_k", "higgs-q4_k_m"]} />
            {rowOf("higgs-engine")}
          </>
        )}
      </Group>
      <Group label={t("settings.roleAsr")}>
        {/* Движок ASR: Parakeet-TDT (GPU, дефолт) / Whisper (локально, CPU) / OpenRouter (облако). */}
        <div className="flex gap-1 mb-1.5">
          {[{ id: "parakeet", label: "Parakeet-TDT", cloud: false }, { id: "whisper", label: "Whisper", cloud: false }, { id: "openrouter", label: "OpenRouter", cloud: true }].map((e) => {
            const asrCloud = selv("or_asr_on") === "1";
            const active = e.cloud ? asrCloud : (!asrCloud && asrEngine === e.id);
            const dis = e.cloud && !hasOrKey;
            return (
              <button key={e.id} title={dis ? t("cloud.needKey") : ""}
                onClick={() => { if (dis) { openSettings("cloud:key"); } else if (e.cloud) { setSel("or_asr_on", "1"); } else { setSel("or_asr_on", "0"); setAsrEngine(e.id); api.setSelection("asr_engine", e.id).catch((er) => { setErr(er instanceof Error ? er.message : String(er)); loadCap(); }); } }}
                className={`flex-1 px-2 py-1.5 rounded-md text-[12px] font-medium border transition-colors ${active ? "border-[var(--color-accent)] bg-[color-mix(in_oklab,var(--color-accent)_14%,transparent)] text-[var(--color-text)]" : "border-[var(--color-border)] text-[var(--color-muted)] hover:text-[var(--color-text)]"} ${dis ? "opacity-50" : ""}`}>
                {e.label}
              </button>
            );
          })}
        </div>
        {/* Локальный ASR (Parakeet/Whisper) — на чём считать: свои табы устройства. */}
        {selv("or_asr_on") !== "1" && <BackendTabs k="asr_backend" />}
        {selv("or_asr_on") === "1" ? (
          <div className={`${orRowCls} space-y-2`}>
            {orModelSelect("asr", "or_asr", t("providers.pickAsrModel"))}
            <div className="text-[11px] text-[var(--color-muted)]">{t("providers.asrCloudHint")}</div>
          </div>
        ) : asrEngine === "parakeet" ? (
          <VariantPicker base="Parakeet-TDT 0.6B v3" ids={["parakeet", "parakeet-ultra-int8", "parakeet-fp32", "parakeet-ultra"]} i18n={ASR_VARIANT_I18N} />
        ) : (
          <>
            {rowOf("whisper-engine")}
            <VariantPicker base={t("settings.whisperModel")} ids={["whisper-tiny", "whisper-base", "whisper-small", "whisper-medium", "whisper-large-v3", "whisper-large-v3-turbo"]} />
            {/* Квант Whisper (compute_type) — точность/скорость вычислений, без отдельной скачки. */}
            <div className="px-2.5 py-2 rounded-lg bg-[var(--color-surface-2)] border border-[var(--color-border)]">
              <div className="flex items-center gap-2.5">
                <span className="w-1.5 h-1.5 rounded-full shrink-0 bg-[var(--color-accent)]" />
                <div className="min-w-0 flex-1">
                  <div className="text-[12px] font-medium truncate">{t("settings.whisperCompute")}</div>
                  <div className="mono text-[10px] text-[var(--color-muted)] truncate">{t("settings.whisperComputeHint")}</div>
                </div>
                <select value={whisperCompute} onChange={(e) => { setWhisperCompute(e.target.value); api.setSelection("whisper_compute", e.target.value).catch(() => {}); }}
                  className="shrink-0 bg-[var(--color-surface)] border border-[var(--color-border)] rounded-md px-2 py-1 text-[11px] mono focus:border-[var(--color-accent)] focus:outline-none">
                  {(cap?.whisper_computes ?? ["int8", "int8_float16", "float16", "int8_float32", "float32"]).map((q) => <option key={q} value={q}>{q}</option>)}
                </select>
              </div>
            </div>
          </>
        )}
      </Group>
      <Group label={t("settings.roleMt")}>
        {/* Перевод и vision — каждый своим провайдером: своя Gemma, локальный сервер или OpenRouter. */}
        <LlmProviders selection={cap?.selection} hasOrKey={hasOrKey} onChanged={loadCap} />
        {needsGemma && (
          <>
            <VariantPicker base="Gemma-4 12B QAT + vision" ids={["gemma", "gemma-q5_0", "gemma-q6_k", "gemma-q8_0"]} />
            {rowOf("llama")}
          </>
        )}
      </Group>
      <Group label={t("settings.roleSep")}>
        {/* На чём считать сепарацию — свои табы (GPU CUDA-сборка / CPU-сборка BSRoformer). */}
        <BackendTabs k="sep_backend" />
        <VariantPicker base="Mel-Band Roformer voc_fv6" ids={["roformer", "roformer-q5", "roformer-q4"]} />
        {selv("sep_backend") === "cpu" ? rowOf("bsroformer-engine-cpu") : rowOf("bsroformer-engine")}
      </Group>
      <Group label={t("settings.roleDiar")}>
        {/* На чём считать диаризацию — свои табы (Nemotron 3 Diarization onnx: CUDA-EP / CPU-провайдер). */}
        <BackendTabs k="diar_backend" />
        {rowOf("sortformer")}
      </Group>
      <Group label={t("settings.roleRuntime")}>{rowOf("onnxruntime")}{(selv("diar_backend") === "gpu" || selv("asr_backend") === "gpu") && rowOf("onnxruntime-gpu")}{(selv("diar_backend") === "gpu" || selv("asr_backend") === "gpu") && rowOf("cudnn")}{rowOf("ffmpeg")}{rowOf("cuda-runtime")}{rowOf("vcruntime")}{rowOf("ocr")}</Group>
      <YtDlpTool installed={!!get("ytdlp")?.installed} row={rowOf("ytdlp")} />
      {/* Производительность / экономия RAM — ВИДИМЫЕ контролы (не авто-магия): против OOM на слабой памяти. */}
      <Group label={t("settings.perfTitle")}>
        {[
          { key: "llama_ubatch", label: t("settings.llamaUbatch"), hint: t("settings.llamaUbatchHint"), opts: cap?.llama_ubatches ?? ["0", "512", "256", "128"], def: "0", fmt: (v: string) => (v === "0" ? t("settings.auto") : v) },
          { key: "higgs_ref_secs", label: t("settings.higgsRef"), hint: t("settings.higgsRefHint"), opts: cap?.higgs_ref_secs_opts ?? ["12", "8", "6", "4"], def: "12", fmt: (v: string) => `${v}${t("settings.sec")}` },
        ].map((row) => {
          const cur = slot(cap?.selection, row.key) ?? row.def;
          return (
            <div key={row.key} className="px-2.5 py-2 rounded-lg bg-[var(--color-surface-2)] border border-[var(--color-border)]">
              <div className="flex items-center gap-2.5">
                <span className="w-1.5 h-1.5 rounded-full shrink-0 bg-[var(--color-muted)]" />
                <div className="min-w-0 flex-1">
                  <div className="text-[12px] font-medium truncate">{row.label}</div>
                  <div className="mono text-[10px] text-[var(--color-muted)] truncate">{row.hint}</div>
                </div>
                <select value={cur} onChange={(e) => { api.setSelection(row.key, e.target.value).then(loadCap).catch((er) => setErr(er instanceof Error ? er.message : String(er))); }}
                  className="shrink-0 bg-[var(--color-surface)] border border-[var(--color-border)] rounded-md px-2 py-1 text-[11px] mono focus:border-[var(--color-accent)] focus:outline-none">
                  {row.opts.map((o) => <option key={o} value={o}>{row.fmt(o)}</option>)}
                </select>
              </div>
            </div>
          );
        })}
      </Group>
    </div>
  );
}

// Пресет под железо: автоопределение GPU/VRAM -> рекомендованные кванты, но юзер применяет любой сам.
// Облачный пресет включает OpenRouter на все стадии — работает даже на слабом ПК без своей GPU.
function PresetsSection({ onApplied }: { onApplied?: () => void }) {
  const { t } = useTranslation();
  const [data, setData] = useState<Awaited<ReturnType<typeof api.hwPresets>> | null>(null);
  const [applied, setApplied] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => { api.hwPresets().then(setData).catch(() => {}); }, []);
  const apply = async (id: string) => {
    setBusy(true);
    try { await api.applyPreset(id); setApplied(id); onApplied?.(); } catch { /* ignore */ }
    setBusy(false);
  };
  const hw = data?.hardware;
  const cur = applied ?? hw?.recommended ?? "";
  const curP = data?.presets.find((p) => p.id === cur);
  return (
    <div className="mb-4 pb-3 border-b border-[var(--color-border)]">
      <div className="text-[11px] uppercase tracking-[0.16em] text-[var(--color-muted)] mb-1.5">{t("hwPreset.title")}</div>
      <div className="px-2.5 py-2 rounded-lg bg-[var(--color-surface-2)] border border-[var(--color-border)]">
        <div className="flex items-center gap-2.5">
          <span className="w-1.5 h-1.5 rounded-full shrink-0 bg-[var(--color-accent)]" />
          <div className="min-w-0 flex-1">
            <div className="text-[12px] font-medium truncate">{hw ? (hw.hasGpu ? t("hwPreset.gpu", { name: hw.gpuName, gb: hw.totalVramGb.toFixed(0) }) : t("hwPreset.noGpu")) : "…"}</div>
            <div className="mono text-[10px] text-[var(--color-muted)] truncate">{curP?.subtitle ?? t("hwPreset.ram", { gb: hw?.totalRamGb.toFixed(0) ?? "?" })}</div>
          </div>
          <select value={cur} onChange={(e) => apply(e.target.value)} disabled={busy}
            className="shrink-0 bg-[var(--color-surface)] border border-[var(--color-border)] rounded-md px-2 py-1 text-[11px] mono focus:border-[var(--color-accent)] focus:outline-none disabled:opacity-50">
            {(data?.presets ?? []).map((p) => <option key={p.id} value={p.id}>{p.title}{p.id === hw?.recommended ? " ★" : ""}</option>)}
          </select>
        </div>
      </div>
    </div>
  );
}

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
      <div role="dialog" aria-modal="true" aria-labelledby="help-title" className="w-[min(92vw,640px)] max-h-[86vh] overflow-y-auto rounded-xl glass-panel anim-pop p-5" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between mb-2">
          <span id="help-title" className="flex items-center gap-2 font-semibold"><HelpCircle size={17} className="text-[var(--color-accent)]" />{t("help.title")}</span>
          <button onClick={onClose} {...named(t("a11y.close"))} className="text-[var(--color-muted)] hover:text-[var(--color-text)]"><X size={16} /></button>
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

// Статус-строка в шапке: текущее действие приложения + разворот по клику в полный журнал (что делалось).
function StatusBar() {
  const { t } = useTranslation();
  const activities = useStore((s) => s.activities);
  const progress = useStore((s) => s.progress);
  const rendering = useStore((s) => s.rendering);
  const jobSteps = useStore((s) => s.jobSteps);
  const [open, setOpen] = useState(false);
  const last = activities[activities.length - 1];
  const busy = rendering || progress.pct != null || (!!progress.stage && !["", "done", "error"].includes(progress.stage));
  const text = busy ? (stageLabel(progress.stage, t, jobSteps) || progress.msg || last?.text || t("status.working")) : (last?.text || t("status.idle"));
  const errored = !busy && last?.kind === "error";
  const fmt = (ms: number) => new Date(ms).toLocaleTimeString();
  return (
    <div className="relative flex-1 min-w-0 flex justify-center px-3">
      <span className="sr-only" role="status" aria-live="polite">{text}</span>
      <button onClick={() => setOpen((o) => !o)} title={t("status.log")} aria-expanded={open}
        className="inline-flex items-center gap-2 max-w-full px-3 py-1 rounded-md text-[12px] text-[var(--color-muted)] hover:text-[var(--color-text)] hover:bg-[var(--color-surface-2)] transition-colors">
        {busy
          ? <Loader2 size={13} className="animate-spin text-[var(--color-accent)] shrink-0" />
          : <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${errored ? "bg-[var(--color-warn)]" : "bg-[var(--color-accent)]"}`} />}
        <span className="truncate">{text}</span>
        {activities.length > 0 && <ChevronDown size={13} className={`shrink-0 transition-transform ${open ? "rotate-180" : ""}`} />}
      </button>
      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
          <div role="log" aria-label={t("status.log")} className="absolute top-full mt-1 left-1/2 -translate-x-1/2 z-50 w-[min(560px,92vw)] max-h-[60vh] overflow-y-auto rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] shadow-xl p-1.5">
            <div className="flex items-center justify-between px-2 py-1 mb-1 border-b border-[var(--color-border)]">
              <span className="text-[11px] uppercase tracking-wide text-[var(--color-muted)] inline-flex items-center gap-1.5"><ScrollText size={13} />{t("status.log")}</span>
              {activities.length > 0 && <button onClick={() => useStore.setState({ activities: [] })} className="text-[11px] text-[var(--color-muted)] hover:text-[var(--color-text)] transition-colors">{t("status.clear")}</button>}
            </div>
            {activities.length === 0 && <div className="text-[11px] text-[var(--color-muted)]/60 py-3 text-center">—</div>}
            {[...activities].reverse().map((a, i) => (
              <div key={i} className="flex items-start gap-2 px-2 py-1 text-[12px]">
                <span className="mono text-[10px] text-[var(--color-muted)]/70 tabnum shrink-0 mt-[3px]">{fmt(a.t)}</span>
                {a.kind === "agent"
                  ? <Bot size={12} aria-label={t("bridge.agent")} className="shrink-0 mt-[3px] text-[var(--color-accent)]" />
                  : <span className={`w-1.5 h-1.5 rounded-full shrink-0 mt-1.5 ${a.kind === "error" ? "bg-[var(--color-warn)]" : a.kind === "done" ? "bg-[var(--color-accent)]" : "bg-[var(--color-muted)]"}`} />}
                <span className={`leading-snug break-words min-w-0 ${a.kind === "error" ? "text-[var(--color-warn)]" : "text-[var(--color-text)]"}`}>{a.text}</span>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

const settingsRequest = (target: string) => (cur: { request: number } | null) => ({ target, request: (cur?.request ?? 0) + 1 });

function TopBar() {
  const { t } = useTranslation();
  // Открытые настройки: цель «раздел[:часть]» и номер запроса — повторный запрос той же цели снова ведёт к ней.
  const [settings, setSettings] = useState<{ target: string; request: number } | null>(null);
  const [help, setHelp] = useState(false);
  const openSettingsAt = (target: string) => setSettings(settingsRequest(target));
  useEffect(() => {
    const open = (e: Event) => setSettings(settingsRequest((e as CustomEvent<string>).detail));
    window.addEventListener(OPEN_SETTINGS_EVENT, open);
    return () => window.removeEventListener(OPEN_SETTINGS_EVENT, open);
  }, []);
  // start over with a new video — the current project stays on disk (reachable via Recent), so no confirm needed
  const newProject = goHome;
  useBridgeCommand("open_settings", (args) => {
    takeArgs("ui_open_settings", args, ["section"]);
    const section = textArg("ui_open_settings", args, "section") ?? "models";
    if (!(SETTINGS_SECTIONS as readonly string[]).includes(section)) throw bridgeError("bad_value", { command: "ui_open_settings", field: "section", expected: SETTINGS_SECTIONS.join(", ") });
    openSettingsAt(section);
    return { open: "settings", section };
  });
  useBridgeCommand("open_help", (args) => { takeArgs("ui_open_help", args, []); setHelp(true); return { open: "help" }; });
  return (
    <header className="flex items-center gap-2 px-5 h-14 border-b border-[var(--color-border)] bg-[var(--color-surface)]">
      <div className="flex items-center gap-3 shrink-0">
        <span className="flex items-center gap-2 font-bold tracking-tight">
          <img src="/favicon.svg" alt="" width={20} height={20} className="rounded-[5px] shadow-[0_0_10px_rgba(198,242,78,0.25)]" />
          {t("app.name")}
          <span className="font-normal text-[11px] text-[var(--color-muted)] tracking-normal" title={t("app.name") + " v" + __APP_VERSION__}>v{__APP_VERSION__}</span>
        </span>
        {/* Режимы редактора телепортируются сюда из Editor (портал), рядом с лого. */}
        <div id="editor-modes-slot" className="flex items-center" />
      </div>
      <StatusBar />
      <div className="flex items-center gap-2 shrink-0">
        <div id="dock-slot" className="flex items-center gap-2 shrink-0" />
        {/* Экспорт-сплит редактора (портал) — статично рядом с «Новый». */}
        <div id="editor-actions-slot" className="flex items-center gap-2" />
        <button onClick={newProject} title={t("manual.newTitle")}
          className="inline-flex flex-col items-center justify-center px-3 py-1 rounded-lg bg-[var(--color-surface-2)] border border-[var(--color-border)] text-[var(--color-text)] hover:border-[var(--color-accent)] hover:text-[var(--color-accent)] transition-colors shadow-sm">
          <span className="text-[12px] font-bold flex items-center gap-1"><Plus size={13} /> {t("nav.new")}</span>
          <span className="text-[9px] text-[var(--color-muted)] font-normal leading-none mt-0.5">{t("manual.newSub")}</span>
        </button>
        <WhatsNew />
        <button onClick={() => setHelp(true)} title={t("help.title")}
          className="p-1.5 rounded-md text-[var(--color-muted)] hover:text-[var(--color-text)] transition-colors"><HelpCircle size={18} /></button>
        <button onClick={() => openSettingsAt("models")} title={t("prefs.title")} aria-label={t("prefs.title")}
          className="p-1.5 rounded-md text-[var(--color-muted)] hover:text-[var(--color-text)] transition-colors"><Settings size={18} /></button>
        <LanguageSwitcher />
      </div>
      {help && <HelpModal onClose={() => setHelp(false)} />}
      {settings !== null && (
        <SettingsModal target={settings.target} request={settings.request} onClose={() => setSettings(null)}
          panes={{ models: <><PresetsSection /><ModelsSection /></>, cloud: <ModelsSection part="cloud" />, network: <ProxySection />, agent: <AgentPanel /> }} />
      )}
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

// Принимаем все распространённые видео-форматы (ffmpeg декодит их все) — не только mp4/mov.
const VIDEO_ACCEPT = "video/*,.mp4,.mov,.mkv,.avi,.webm,.m4v,.flv,.wmv,.ts,.mpg,.mpeg,.3gp,.ogv,.mts,.m2ts,.vob,.f4v";
// Аудио-режим: вход без видео (пачка WAV/mp3 → пачка озвученных WAV). ffmpeg декодит всё.
const AUDIO_ACCEPT = "audio/*,.wav,.mp3,.flac,.m4a,.aac,.ogg,.opus,.wma,.aiff,.aif,.alac";
const MEDIA_ACCEPT = `${VIDEO_ACCEPT},${AUDIO_ACCEPT}`;
const AUDIO_EXT = /\.(wav|mp3|flac|m4a|aac|ogg|opus|wma|aiff|aif|alac)$/i;
// Файл — аудио (без видеодорожки)? По MIME или расширению. Видео-MIME имеет приоритет (напр. .ogv/.m4v).
const isAudioFile = (f: File) => !f.type.startsWith("video/") && (f.type.startsWith("audio/") || AUDIO_EXT.test(f.name));

// Стиль перевода (#112): пресет -> АНГЛИЙСКАЯ инструкция для Геммы (уходит на бэк как translate_style).
// "" = обычный (без указаний). "custom" -> берётся свой текст из textarea. Формулировки на английском.
const TR_STYLE_PRESETS: Record<string, string> = {
  "": "",
  technical: "Technical register: translate terminology precisely, keep product names and units as-is, no colloquialisms.",
  literary: "Literary register: natural expressive language, idiomatic phrasing, preserve tone and imagery.",
  casual: "Casual conversational register: everyday spoken language, contractions, simple words.",
};
const TR_STYLE_IDS = ["", "technical", "literary", "casual", "custom"] as const;
// Итоговый текст стиля на бэк: пресет -> его текст; "custom" -> свой текст (обрезанный).
function resolveTrStyle(choice: string, custom: string): string {
  return choice === "custom" ? custom.trim() : (TR_STYLE_PRESETS[choice] ?? "");
}
// Красивое имя голоса для списка: срезаем префиксы RU_Male_/RU_Female_ и подчёркивания (значение — полное имя).
const prettyVoice = (n: string) => n.replace(/^[A-Z]{2}_(Female|Male)_/i, "").replace(/_/g, " ");

// Динамический список слотов голосов одного пола (#114): строки [селект голоса][▶ сэмпл][× удалить] + «+ добавить».
// Порядок строк = приоритет спикеров. Значение слота — полное имя файла без расширения (как в GET /voices).
function VoiceSlotList({ title, voices, slots, onChange }: {
  title: string; voices: string[]; slots: string[]; onChange: (v: string[]) => void;
}) {
  const { t } = useTranslation();
  const [playing, setPlaying] = useState<number | null>(null);   // индекс проигрываемого слота
  const audioRef = useRef<HTMLAudioElement | null>(null);
  useEffect(() => () => { audioRef.current?.pause(); }, []);
  const toggle = (i: number, name: string) => {
    audioRef.current?.pause();
    if (playing === i) { setPlaying(null); return; }              // повторный клик = стоп
    if (!name) return;
    const a = new Audio(api.voiceSampleUrl(name)); audioRef.current = a; setPlaying(i);
    // Отложенные колбэки старого аудио не должны трогать playing после старта нового — сверяемся с текущим.
    a.onended = () => { if (audioRef.current === a) setPlaying(null); };
    a.play().then(() => { if (audioRef.current === a) setPlaying(i); }).catch(() => { if (audioRef.current === a) setPlaying(null); });
  };
  const opts = voices.map((v) => ({ value: v, label: prettyVoice(v), search: v }));
  const setAt = (i: number, v: string) => onChange(slots.map((s, j) => (j === i ? v : s)));
  const removeAt = (i: number) => { audioRef.current?.pause(); setPlaying(null); onChange(slots.filter((_, j) => j !== i)); };
  const add = () => onChange([...slots, voices[0] ?? ""]);
  return (
    <div>
      <div className="text-[10px] uppercase tracking-[0.1em] text-[var(--color-muted)] mb-1">{title}</div>
      <div className="space-y-1">
        {slots.map((name, i) => (
          <div key={i} className="flex items-center gap-1">
            <Combobox value={name} onChange={(v) => setAt(i, v)} options={opts}
              placeholder={t("voice.search")} noResults={t("voice.noMatch")} size="sm" className="flex-1 min-w-0" />
            <button onClick={() => toggle(i, name)} title={t("voice.preview")}
              className="shrink-0 p-1 rounded-md text-[var(--color-muted)] hover:text-[var(--color-accent)]">{playing === i ? <Pause size={13} /> : <Play size={13} />}</button>
            <button onClick={() => removeAt(i)} title={t("voiceSlots.remove")}
              className="shrink-0 p-1 rounded-md text-[var(--color-muted)] hover:text-[#ef4444]"><X size={13} /></button>
          </div>
        ))}
      </div>
      <button onClick={add} disabled={voices.length === 0}
        className="mt-1 inline-flex items-center gap-1 text-[11px] text-[var(--color-accent-2)] hover:text-[var(--color-accent)] disabled:opacity-40">
        <Plus size={12} />{t("voiceSlots.add")}</button>
    </div>
  );
}

// Аккордеон стартового экрана (#118): свёрнутая по умолчанию секция вторичных опций. Заголовок-кнопка
// в стиле секций (uppercase 11px) + краткий статус справа и шеврон, поворот на 180° при раскрытии.
// Плавность высоты через grid-rows 0fr→1fr (без ручного замера max-height); контент рендерится всегда,
// чтобы стейты внутри не сбрасывались при сворачивании.
function Accordion({ title, subtitle, defaultOpen = false, children }: {
  title: string; subtitle?: string; defaultOpen?: boolean; children: React.ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="mt-2 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)]">
      <button type="button" onClick={() => setOpen((o) => !o)}
        className="w-full flex items-center gap-2 px-2.5 py-2 text-left">
        <span className="text-[11px] uppercase tracking-[0.1em] text-[var(--color-muted)] shrink-0">{title}</span>
        {subtitle && !open && <span className="text-[11px] text-[var(--color-muted)]/70 truncate min-w-0 flex-1">{subtitle}</span>}
        <ChevronDown size={15} className={`ml-auto shrink-0 text-[var(--color-muted)] transition-transform duration-200 ${open ? "rotate-180" : ""}`} />
      </button>
      <div className={`grid transition-[grid-template-rows] duration-200 ${open ? "grid-rows-[1fr]" : "grid-rows-[0fr]"}`}>
        <div className="overflow-hidden">
          <div className="px-2.5 pb-2.5 pt-0.5">{children}</div>
        </div>
      </div>
    </div>
  );
}

function DropZone() {
  const { t, i18n } = useTranslation();
  const s = useStore();
  const inputRef = useRef<HTMLInputElement>(null);
  const batchRef = useRef<HTMLInputElement>(null);
  const [over, setOver] = useState(false);
  // Дефолты запуска хранит сервис (GET/PATCH /settings/launch): поля формы ниже — их рабочая копия,
  // каждое изменение уходит на сервер, а с сервера форма заполняется при открытии.
  const [launch] = useState(() => createLaunchSaver(api.saveLaunchDefaults,
    (e) => useStore.getState().pushActivity(t("launch.saveFailed", { error: e instanceof Error ? e.message : String(e) }), "error")));
  const saveLaunch = (patch: Partial<LaunchDefaults>) => launch.queue(patch);
  useEffect(() => () => { void launch.flush(); }, [launch]);
  const [tgt, setTgt] = useState<string>((i18n.language as string) || "ru");   // translate TO (default = UI lang)
  const [src, setSrc] = useState("auto");                                       // translate FROM (auto-detect)
  const [speakerCount, setSpeakerCount] = useState(0);
  const chooseSpeakerCount = (count: number) => { setSpeakerCount(count); saveLaunch({ speaker_count: count }); };
  const chooseTgt = (lang: string) => { setTgt(lang); saveLaunch({ tgt_lang: lang }); };
  const chooseSrc = (lang: string) => { pickSrc(lang); saveLaunch({ src_lang: lang }); };
  const [asrNote, setAsrNote] = useState<string | null>(null);                  // «Parakeet не знает язык → переключили на Whisper»
  // Выбор источника: если язык вне 25 европейских Parakeet — авто-переключаем ASR на Whisper (99 языков)
  // с уведомлением. Текущий движок спрашиваем у бэкенда В МОМЕНТ выбора (не кэш с маунта): юзер мог
  // сменить движок в «Моделях», DropZone при этом не ремаунтится — кэш дал бы решение по устаревшему
  // значению и Parakeet молча поехал бы по языку, которого не знает (ревью-находка).
  const srcReq = useRef("");                                                   // актуальность async-ответа: быстрый повторный выбор языка не должен воскрешать ноту прошлого
  function pickSrc(lang: string) {
    setSrc(lang);
    srcReq.current = lang;
    if (lang === "auto" || PARAKEET_LANGS.has(lang)) {
      setAsrNote(null);
      return;
    }
    api.capabilities()
      .then((c) => {
        if (srcReq.current !== lang) return; // юзер уже выбрал другой язык — ответ устарел
        if ((c.selection?.asr_engine ?? "parakeet") === "parakeet") {
          api.setSelection("asr_engine", "whisper").then(
            () => { if (srcReq.current === lang) setAsrNote(DUB_LANGS.find((l) => l.code === lang)?.name ?? lang); },
            (e: unknown) => {
              if (srcReq.current === lang) setAsrNote(null);
              useStore.getState().pushActivity(t("prefs.saveFailed", { error: e instanceof Error ? e.message : String(e) }), "error");
            },
          );
        } else {
          setAsrNote(null);
        }
      })
      .catch(() => { if (srcReq.current === lang) setAsrNote(null); });
  }
  const [file, setFile] = useState<File | null>(null);                          // staged video — analyzed on Start, not on drop
  const [subsFile, setSubsFile] = useState<File | null>(null);                  // опц. готовые субтитры (SRT/ASS) -> текст+тайминг вместо ASR
  const [subsTranslated, setSubsTranslated] = useState(false);                  // сабы уже на языке перевода -> tgt из них, MT пропустить (Даб Студио только озвучивает)
  const [subsAlign, setSubsAlign] = useState(false);
  // Видео, скачанное по ссылке, — уже проект на сервере. Ждёт здесь, как выбранный файл: «Начать обработку»
  // запускает по нему analyze с настройками этого экрана, «Ручной режим» открывает его в редакторе.
  const [fetched, setFetched] = useState<{ pid: string; title: string; duration: number | null; audioOnly: boolean; subsLang: string | null } | null>(null);
  const pickFile = (f: File) => { setFile(f); setFetched(null); };
  const takeFetched = (f: UrlFetch & { pid: string }) => {
    setFile(null); setSubsFile(null); setSubsTranslated(false); setSubsAlign(false);
    setFetched({ pid: f.pid, title: f.title ?? f.url, duration: f.duration, audioOnly: f.quality === "audio", subsLang: f.subsImported ? f.subsLang : null });
  };
  // Композируемые опции обработки (независимы, любые комбинации). audio = аудио-выход; subs = содержимое
  // субтитров; burn = вжигать ли их на видео; funnyOn+funny = шуточный ремикс (сочетается с дубляжом/голосом).
  const [audio, setAudio] = useState<LaunchDefaults["audio"]>("dub");
  const [subs, setSubs] = useState<LaunchDefaults["subs"]>("translate");
  const [burn, setBurn] = useState(true);
  const chooseAudio = (v: LaunchDefaults["audio"]) => { setAudio(v); saveLaunch({ audio: v }); };
  const chooseSubs = (v: LaunchDefaults["subs"]) => { setSubs(v); saveLaunch({ subs: v }); };
  const chooseBurn = (v: boolean) => { setBurn(v); saveLaunch({ burn: v }); };
  const [detectText, setDetectText] = useState(false);                          // OCR-детекция вшитого текста (блюр/локализация титров). Дорогая на 4K -> ПО УМОЛЧАНИЮ ВЫКЛ (юзеры жаловались, что дубляж без сабов всё равно сканирует кадры); кто хочет блюр вшитых субтитров — включает галочкой.
  const chooseDetect = (v: boolean) => { setDetectText(v); saveLaunch({ detect_text: v }); };
  // Кастинг персонажей (#115): доп. проход по кадрам (детект лиц + эмбеддинги + active-speaker) -> база
  // персонажей с аватарами/голосами. Опционально, дорого на длинном видео -> ПО УМОЛЧАНИЮ ВЫКЛ.
  const [castingOn, setCastingOn] = useState(false);
  const setCastingSaved = (v: boolean) => { setCastingOn(v); saveLaunch({ casting: v }); };
  // Готовый кастинг из библиотеки (#115): slug профиля -> уходит в analyze(casting_ref=). Пусто = не применять.
  // Глобальный дефолт (как стиль перевода/громкость) — чтобы серию роликов дубить одним кастингом.
  const [castingRef, setCastingRef] = useState("");
  const setCastingRefSaved = (v: string) => { setCastingRef(v); saveLaunch({ casting_ref: v }); };
  const [deletingCast, setDeletingCast] = useState<string | null>(null);       // профиль кастинга в диалоге удаления
  // Тип контента кастинга (#115): real (SCRFD+LVFace) | anime (детектор рисованных лиц + CCIP).
  const [contentType, setContentType] = useState<LaunchDefaults["content_type"]>("auto");
  const setContentTypeSaved = (v: LaunchDefaults["content_type"]) => { setContentType(v); saveLaunch({ content_type: v }); };
  // Список профилей библиотеки — грузим лениво, когда галка кастинга включена (не засорять UI при выкл.).
  const [castLib, setCastLib] = useState<{ slug: string; name: string; char_count: number }[]>([]);
  const refreshCastLib = () => api.castingLibrary().then((r) => setCastLib(r.casts)).catch(() => {});
  useEffect(() => { if (castingOn) refreshCastLib(); }, [castingOn]);
  useChanged(SETTINGS_CHANGED, () => { if (castingOn) refreshCastLib(); });
  const [funnyOn, setFunnyOn] = useState(false);
  const [funny, setFunny] = useState("");                                       // Gemma rewrite instruction (тема ремикса)
  // Громкость оригинала под переводом (voiceover), стартовый выбор -> применяется ко всем создаваемым проектам
  // (дефолт -12 dB, broadcast-практика).
  const [voGain, setVoGain] = useState(-12);
  const setVoGainSaved = (v: number) => { setVoGain(v); saveLaunch({ vo_gain_db: v }); };
  // Стиль перевода (#112): выбор пресета + свой текст — глобальный дефолт для будущих запусков.
  const [trStyle, setTrStyle] = useState<LaunchDefaults["tr_style"]>("");
  const [trStyleCustom, setTrStyleCustom] = useState("");
  const setTrStyleSaved = (v: LaunchDefaults["tr_style"]) => { setTrStyle(v); saveLaunch({ tr_style: v }); };
  const setTrStyleCustomSaved = (v: string) => { setTrStyleCustom(v); saveLaunch({ tr_style_custom: v }); };
  // Сохранить оригинальную дорожку (#113): 2-я аудиодорожка + контейнер вывода (mp4|mkv). Персист.
  // Дакинг фона под дубляжом — опция дубляжа (active.json duck_on), ВЫКЛ по умолчанию (не всем нужен).
  const [duckOn, setDuckOn] = useState(false);
  useEffect(() => { api.capabilities().then((c) => setDuckOn(c.selection?.duck_on === "1")).catch(() => {}); }, []);
  const setDuckSaved = (v: boolean) => { setDuckOn(v); api.setSelection("duck_on", v ? "1" : "0").catch(() => {}); };
  // Блюр-подложка под сожжёнными субтитрами — опция (не всем нужна), дефолт ВКЛ; патчится в проект после analyze.
  const [subBlur, setSubBlur] = useState(true);
  const setSubBlurSaved = (v: boolean) => { setSubBlur(v); saveLaunch({ sub_blur: v }); };
  const [keepOrig, setKeepOrig] = useState(false);
  const [container, setContainer] = useState<LaunchDefaults["container"]>("mp4");
  const setKeepOrigSaved = (v: boolean) => { setKeepOrig(v); saveLaunch({ keep_orig: v }); };
  const setContainerSaved = (v: LaunchDefaults["container"]) => { setContainer(v); saveLaunch({ container: v }); };
  // Голоса из библиотеки (#114): режим клон|library + два списка слотов (порядок = приоритет).
  const [voiceSrc, setVoiceSrc] = useState<LaunchDefaults["voice_src"]>("clone");
  const [slotsM, setSlotsM] = useState<string[]>([]);
  const [slotsF, setSlotsF] = useState<string[]>([]);
  const setVoiceSrcSaved = (v: LaunchDefaults["voice_src"]) => { setVoiceSrc(v); saveLaunch({ voice_src: v }); };
  const setSlotsMSaved = (v: string[]) => { setSlotsM(v); saveLaunch({ voice_slots_m: v }); };
  const setSlotsFSaved = (v: string[]) => { setSlotsF(v); saveLaunch({ voice_slots_f: v }); };
  // Сохранённый выбор с сервера (старый выбор окна переносится туда один раз). Не прочитался — форма
  // остаётся на встроенных дефолтах, и журнал говорит почему.
  useEffect(() => {
    let alive = true;
    loadWithMigration({ load: api.launchDefaults, save: api.saveLaunchDefaults }, localStorage)
      .then(({ defaults: d }) => {
        if (!alive) return;
        setAudio(d.audio); setSubs(d.subs); setBurn(d.burn); setDetectText(d.detect_text);
        if (d.tgt_lang) setTgt(d.tgt_lang);
        pickSrc(d.src_lang);
        setSpeakerCount(d.speaker_count ?? 0);
        setCastingOn(d.casting); setCastingRef(d.casting_ref); setContentType(d.content_type);
        setVoGain(d.vo_gain_db); setTrStyle(d.tr_style); setTrStyleCustom(d.tr_style_custom);
        setSubBlur(d.sub_blur); setKeepOrig(d.keep_orig); setContainer(d.container);
        setVoiceSrc(d.voice_src); setSlotsM(d.voice_slots_m); setSlotsF(d.voice_slots_f);
      })
      .catch((e: unknown) => useStore.getState().pushActivity(t("launch.loadFailed", { error: e instanceof Error ? e.message : String(e) }), "error"));
    return () => { alive = false; };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
  const [voiceLib, setVoiceLib] = useState<string[]>([]);                        // имена голосов из GET /voices (для селектов слотов)
  useEffect(() => { api.voices().then((r) => setVoiceLib(r.voices)).catch(() => {}); }, []);
  useChanged(VOICES_CHANGED, () => {
    api.voices().then((r) => setVoiceLib(r.voices))
      .catch((err) => useStore.getState().pushActivity(t("bridge.syncFailed", { error: String(err) }), "error"));
  });
  const [preview, setPreview] = useState<string | null>(null);                  // objectURL превью выбранного видео (первый кадр)
  const audioOnly = file ? isAudioFile(file) : !!fetched?.audioOnly;            // вход без видео -> режим «только аудио»
  // Субтитры проекта: выбранный вместе с видео файл или субтитры площадки, легшие в проект при загрузке по ссылке.
  const hasSubs = !audioOnly && (fetched ? !!fetched.subsLang : !!subsFile);
  useEffect(() => {                                                             // создаём/освобождаем objectURL под выбранный файл
    if (!file) { setPreview(null); return; }
    const url = URL.createObjectURL(file);
    setPreview(url);
    return () => URL.revokeObjectURL(url);
  }, [file]);

  // «Недавние проекты»: всё уже автосохранено в workspace/<pid>/ (каждая правка = PATCH). Здесь тянем
  // список и даём открыть прошлый проект в один клик. Автообновление URL (?pid=) — чтобы перезагрузка держала.
  const [recent, setRecent] = useState<ProjectListing[]>([]);
  const [allProjects, setAllProjects] = useState(false);                        // экран «Все проекты»
  const [deleting, setDeleting] = useState<{ pid: string; video: string } | null>(null);   // проект в диалоге удаления
  useEffect(() => {
    api.listProjects().then((r) => setRecent(r.projects))
      .catch((e: unknown) => useStore.getState().pushActivity(t("projects.loadFailed", { error: e instanceof Error ? e.message : String(e) }), "error"));
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
  useChanged(PROJECTS_CHANGED, () => {
    api.listProjects().then((r) => setRecent(r.projects))
      .catch((err: unknown) => useStore.getState().pushActivity(t("bridge.syncFailed", { error: err instanceof Error ? err.message : String(err) }), "error"));
  });
  // Удалить проект: DELETE /projects/<pid> стирает workspace/<pid>; из списка убираем только после ответа
  // сервера, ошибка остаётся в диалоге подтверждения.
  const deleteProject = async (pid: string) => {
    await api.deleteProject(pid);
    setRecent((r) => r.filter((x) => x.pid !== pid));
  };
  const deleteRecent = (e: React.MouseEvent, pid: string, video: string) => {
    e.stopPropagation();
    setDeleting({ pid, video });
  };
  const rtf = new Intl.RelativeTimeFormat((i18n.language as string) || "en", { numeric: "auto" });
  function fmtAgo(sec: number) {
    const min = (sec * 1000 - Date.now()) / 60000;                              // отрицательное = в прошлом
    if (Math.abs(min) < 60) return rtf.format(Math.round(min), "minute");
    if (Math.abs(min) < 1440) return rtf.format(Math.round(min / 60), "hour");
    return rtf.format(Math.round(min / 1440), "day");
  }
  async function openProject(pid: string) {
    try {
      await openProjectIn(pid);                                                 // покадровое превью; ?pid= в URL — перезагрузка вернёт проект
      playSfx("success");
    } catch (e) {
      useStore.getState().pushActivity(t("projects.openFailed", { error: e instanceof Error ? e.message : String(e) }), "error");
      playSfx("error");
    }
  }

  async function runManual() {
    if (fetched) { await openProject(fetched.pid); return; }
    if (!file) return;
    try {
      useStore.getState().pushActivity(t("manual.starting"), "work");
      const { project_id } = await api.createProject(file, isAudioFile(file) ? null : subsFile);
      s.setPid(project_id);
      const proj = await api.getProject(project_id);
      s.setProject(proj);
      s.setRendered(false);
      s.setStage("editor");
      window.history.pushState(null, "", `?pid=${project_id}`);
      useStore.getState().pushActivity(t("manual.done"), "done");
      playSfx("success");
    } catch (err) {
      useStore.getState().pushActivity(String(err), "error");
      playSfx("error");
    }
  }

  async function run() {
    const source: { pid: string } | { file: File } | null = fetched ? { pid: fetched.pid } : file ? { file } : null;
    if (!source) return;
    s.setStage("analyzing");
    s.setAudioOnly(audioOnly);               // «Анализируем аудио» вместо «видео» для аудио-входа
    // Шаги степпера — только те, что реально будут в ЭТОЙ джобе (жалоба: «Находим текст на экране»
    // при выключенной детекции; «Переводим/Генерируем озвучку» в режиме транскрипта).
    {
      // subs здесь — ЭФФЕКТИВНЫЙ (как eSubs ниже): transcribe-режим форсит субтитры оригинала,
      // перевода в нём нет, что бы ни стояло в сыром стейте селектора.
      const effSubs = audioOnly ? "none" : audio === "transcribe" ? "transcribe" : subs;
      const wantTranslate = audio === "dub" || audio === "voiceover" || effSubs === "translate" || effSubs === "bilingual" || (funnyOn && !!funny.trim());
      const wantVoice = audio === "dub" || audio === "voiceover";
      const steps = ["download", "separating", "diarizing", "recognizing"];
      if (wantTranslate) steps.push("translating");
      if (wantVoice) steps.push("voicing");
      if (!audioOnly && detectText) steps.push("locating");
      // #115: кастинг гоняем только когда галка реально видна на старте (showCasting = !audioOnly && isVoiced),
      // иначе персист castingOn=1 из прошлого дубляжа гнал бы детект лиц впустую в nodub/subtitles/transcribe.
      if (!audioOnly && wantVoice && castingOn) steps.push("casting");   // доп. проход по кадрам — детект персонажей
      // «Собираем видео» — только когда run() реально гонит рендер (dub/voiceover); в остальных
      // режимах сборка происходит позже на Экспорте, и шаг висел бы серым навсегда (ревью).
      if (wantVoice) steps.push("assembling");
      s.setJobSteps(steps);
    }
    s.setProgress("", "", null);             // fresh stepper for this run
    s.clearResumed();
    try {
      const project_id = "pid" in source ? source.pid
        : (await api.createProject(source.file, isAudioFile(source.file) ? null : subsFile)).project_id;   // сабы — только для видео
      s.setPid(project_id);
      // Стиль перевода (#112): передаём ПАРАМЕТРОМ analyze (patch до analyze невозможен — project.json ещё
      // не создан; стиль читается стадией перевода ВНУТРИ analyze).
      const trStyleText = resolveTrStyle(trStyle, trStyleCustom);
      // subtitles = ОРИГИНАЛ: исходная дорожка + субтитры на языке оригинала (без дубляжа, без перевода);
      // voiceover = закадровый (перевод+TTS, оригинал слышно приглушённым); transcribe = транскрипт+диаризация.
      // Композируемо: аудио-выход, содержимое субтитров, шуточный ремикс — независимы.
      const eMode = audio;                                          // nodub | dub | voiceover | transcribe
      const eSubs = audioOnly ? "none" : audio === "transcribe" ? "transcribe" : subs;   // none | transcribe(оригинал) | translate
      const eRewrite = funnyOn && (audio === "dub" || audio === "voiceover") ? funny.trim() : "";
      // Транскрипт всегда вжигает субтитры (иначе транскрипт-режим дал бы видео без текста): чекбокс
      // «Вжигать» для transcribe скрыт, поэтому форсируем burn=true, не полагаясь на его прежнее значение.
      // Аудио-режим: субтитры/бёрн/OCR не нужны (нет видео) -> off.
      const eBurn = audioOnly ? false : audio === "transcribe" ? true : burn;
      // #115: кастинг только при видео-дубляже (как showCasting на старте); иначе персист castingOn=1
      // ушёл бы с analyze в nodub/subtitles/transcribe и бэк впустую гонял бы детект лиц.
      const effCasting = !audioOnly && (audio === "dub" || audio === "voiceover") && castingOn;
      // Готовый кастинг из библиотеки применяем только когда кастинг реально включён (та же видимость, что у галки).
      const effCastingRef = effCasting ? castingRef : "";
      const effContentType = effCasting ? contentType : "real";
      // Настройки после анализа (громкость оригинала, блюр-подложка, 2-я дорожка #113, голоса из библиотеки
      // #114) сервер кладёт на проект в конце анализа и хранит в job.json: «Продолжить» повторит их же.
      const voiced = audio === "dub" || audio === "voiceover";
      const post: AnalyzePost = {
        voGain: audio === "voiceover" ? voGain : undefined,
        subBlur: eBurn && eSubs !== "none" && !audioOnly ? subBlur : undefined,
        keepOriginal: keepOrig && !audioOnly && voiced ? { container } : undefined,
        voiceSlots: voiceSrc === "library" && voiced && (slotsM.length || slotsF.length) ? { male: slotsM, female: slotsF } : undefined,
      };
      const { job_id } = await api.analyze(project_id, tgt, eMode, src, eSubs, eRewrite, eBurn, audioOnly ? false : detectText, hasSubs && subsTranslated, trStyleText, effCasting, effCastingRef, effContentType, hasSubs && !subsTranslated && subsAlign, post, speakerCount);
      // Ошибка -> «Продолжить» с места остановки, не сброс. Для dub/voiceover озвучка готовится здесь же, на
      // экране загрузки (rendered остаётся false: /dub отдаёт готовый дуб, кадры — покадровое превью).
      await finishAnalyze(project_id, await watchWithResume(project_id, "analyze", job_id));
      s.setStage("editor"); playSfx("success");
    } catch (err) {
      if (err instanceof JobCancelledError) { s.setProgress("", "", null); s.setStage("empty"); return; }
      s.setProgress("error", String(err), null);  // surface backend failure instead of hanging on "analyzing"
      s.setStage("empty"); playSfx("error");
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
            <Feature icon={AudioLines} title={t("actions.dub")} desc={t("hero.f2")} delay={0.12} />
            <Feature icon={Mic2} title={t("mode.voiceover")} desc={t("mode.voiceover_desc")} delay={0.18} />
            <Feature icon={Captions} title={t("mode.subtitles")} desc={t("mode.subtitles_desc")} delay={0.24} />
            <Feature icon={Sparkles} title={t("actions.funny")} desc={t("hero.f3")} delay={0.3} />
            <Feature icon={FileText} title={t("actions.transcribe")} desc={t("hero.f4")} delay={0.36} />
          </div>
          {recent.length > 0 && (
            <div className="mt-8">
              <div className="text-[11px] uppercase tracking-[0.14em] text-[var(--color-muted)] mb-2.5">{t("recent.title")}</div>
              <div className="space-y-1.5 max-h-72 overflow-y-auto pr-1 -mr-1">
                {recent.slice(0, 8).map((p) => (
                  <div key={p.pid} data-mcp-context={`project ${p.pid}: ${p.video}`}
                    className="group relative flex items-center rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] hover:bg-[var(--color-surface-2)] hover:border-[#3a414c] transition-colors">
                    <button onClick={() => openProject(p.pid)}
                      className="min-w-0 flex-1 flex items-center gap-3 p-1.5 text-left">
                      {p.audio_only
                        ? <div className="w-16 h-10 shrink-0 rounded-md grid place-items-center bg-[var(--color-surface-2)] text-[var(--color-accent)]"><AudioLines size={16} /></div>
                        : <img src={api.originalUrl(p.pid, Math.min(1, (p.duration || 3) / 3))} alt="" loading="lazy" className="w-16 h-10 shrink-0 rounded-md object-cover bg-black/40" />}
                      <div className="min-w-0 flex-1">
                        <div className="text-[13px] font-medium truncate">{p.video}</div>
                        <div className="mt-0.5 text-[11px] text-[var(--color-muted)] flex items-center gap-1.5">
                          <span className="uppercase font-semibold text-[var(--color-accent-2)]">{p.tgt_lang}</span>
                          <span>·</span><span className="truncate">{p.mode in MODE_KEYS ? t(MODE_KEYS[p.mode as keyof typeof MODE_KEYS]) : p.mode}</span>
                          <span>·</span><span className="shrink-0">{fmtAgo(p.mtime)}</span>
                          {p.done && <Check size={12} className="text-[var(--color-accent)] shrink-0" />}
                          <JobStateLabel p={p} />
                        </div>
                      </div>
                    </button>
                    <ContinueJobButton p={p} onOpen={openProject} />
                    <button onClick={(e) => deleteRecent(e, p.pid, p.video)} title={t("recent.delete")}
                      className="shrink-0 mr-1 p-1.5 rounded-md text-[var(--color-muted)] opacity-0 group-hover:opacity-100 hover:text-[#ef4444] hover:bg-white/5 transition"><Trash2 size={15} /></button>
                  </div>
                ))}
              </div>
              <button type="button" onClick={() => setAllProjects(true)}
                className="mt-2 inline-flex items-center gap-1.5 text-[12px] text-[var(--color-accent-2)] hover:text-[var(--color-accent)] transition-colors">
                <FolderOpen size={13} />{t("projects.all", { n: recent.length })}
              </button>
            </div>
          )}
        </div>

        <motion.div initial={{ opacity: 0, scale: 0.98 }} animate={{ opacity: 1, scale: 1 }} transition={{ duration: 0.5, ease: EASE, delay: 0.1 }}>
          <div
            onDragOver={(e) => { e.preventDefault(); setOver(true); }}
            onDragLeave={() => setOver(false)}
            onDrop={(e) => { e.preventDefault(); setOver(false); const f = e.dataTransfer.files?.[0]; if (f) pickFile(f); }}
            onClick={() => inputRef.current?.click()}
            className={`group relative aspect-[4/3] rounded-2xl border grid place-items-center cursor-pointer overflow-hidden transition-all duration-200
              ${over ? "border-[var(--color-accent)] bg-[color-mix(in_oklab,var(--color-accent)_9%,var(--color-surface))] shadow-[0_0_0_4px_color-mix(in_oklab,var(--color-accent)_18%,transparent)]"
                     : "border-[var(--color-border)] bg-[var(--color-surface)] hover:bg-[var(--color-surface-2)] hover:border-[#3a414c]"}`}
          >
            {file && preview && !audioOnly && (   // превью выбранного видео (первый кадр) заполняет рамку; скрим — для читаемости текста
              <>
                <video src={preview} muted playsInline preload="metadata" className="absolute inset-0 w-full h-full object-cover" />
                <div className="absolute inset-0 bg-black/45" />
              </>
            )}
            {fetched && !fetched.audioOnly && (
              <>
                <img src={api.originalUrl(fetched.pid, Math.min(1, (fetched.duration || 3) / 3))} alt="" className="absolute inset-0 w-full h-full object-cover" />
                <div className="absolute inset-0 bg-black/45" />
              </>
            )}
            <span className={`${corner} top-3 left-3 border-l border-t`} />
            <span className={`${corner} top-3 right-3 border-r border-t`} />
            <span className={`${corner} bottom-3 left-3 border-l border-b`} />
            <span className={`${corner} bottom-3 right-3 border-r border-b`} />
            <div className="relative z-10 text-center px-6">
              <div className={`mx-auto grid place-items-center w-16 h-16 rounded-2xl border transition-all duration-200
                ${over ? "bg-[var(--color-accent)] text-[var(--color-on-accent)] border-transparent" : (file && preview) || fetched ? "bg-[var(--color-accent)] text-[var(--color-on-accent)] border-transparent shadow-lg" : "bg-[var(--color-surface-2)] text-[var(--color-accent)] border-[var(--color-border)] group-hover:scale-105"}`}>
                {file || fetched ? <Check size={26} strokeWidth={2.5} /> : <Upload size={26} strokeWidth={2} />}
              </div>
              {fetched ? (
                <>
                  <div className={`mt-5 text-lg font-semibold break-words line-clamp-3 px-2 ${fetched.audioOnly ? "" : "text-white drop-shadow"}`}>{fetched.title}</div>
                  <div className={`mt-1.5 text-sm ${fetched.audioOnly ? "text-[var(--color-muted)]" : "text-white/80"}`}>{t("url.phase.done")} · {t("drop.change")}</div>
                  {audioOnly && (
                    <div className="mt-3 inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-[color-mix(in_oklab,var(--color-accent)_16%,transparent)] text-[var(--color-accent)] text-[12px] font-medium">
                      <AudioLines size={13} /> {t("drop.audioMode")}
                    </div>
                  )}
                </>
              ) : file ? (
                <>
                  <div className={`mt-5 text-lg font-semibold break-all px-2 ${preview && !audioOnly ? "text-white drop-shadow" : ""}`}>{file.name}</div>
                  <div className={`mt-1.5 text-sm ${preview && !audioOnly ? "text-white/80" : "text-[var(--color-muted)]"}`}>{(file.size / 1048576).toFixed(1)} {t("units.mb")} · {t("drop.change")}</div>
                  {audioOnly && (
                    <div className="mt-3 inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-[color-mix(in_oklab,var(--color-accent)_16%,transparent)] text-[var(--color-accent)] text-[12px] font-medium">
                      <AudioLines size={13} /> {t("drop.audioMode")}
                    </div>
                  )}
                </>
              ) : (
                <>
                  <div className="mt-5 text-lg font-semibold">{t("drop.title")}</div>
                  <div className="mt-1.5 text-sm text-[var(--color-muted)]">{t("drop.hint")}</div>
                  <span className="mt-5 inline-flex items-center gap-2 px-4 py-2 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-sm font-semibold">
                    {t("drop.browse")} <ArrowRight size={16} /></span>
                </>
              )}
            </div>
          </div>
          <UrlImport staged={fetched ? fetched.pid : null} onReady={takeFetched} />
          <div className="mt-3.5 flex items-center justify-center gap-2 text-[12px]">
            <Languages size={14} className="text-[var(--color-accent-2)]" />
            <Combobox value={src} onChange={chooseSrc}
              options={langOptions(DUB_LANGS, i18n.language, [{ value: "auto", label: t("settings.auto"), search: "auto" }])}
              placeholder={t("voice.langSearch")} noResults={t("voice.noMatch")} size="sm" className="w-[130px]" />
            <ArrowRight size={12} className="text-[var(--color-muted)]" />
            <Combobox value={tgt} onChange={chooseTgt}
              options={langOptions(DUB_LANGS, i18n.language)}
              placeholder={t("voice.langSearch")} noResults={t("voice.noMatch")} size="sm" className="w-[130px]" />
          </div>
          {asrNote
            ? <p className="mt-1 text-center text-[10px] text-[var(--color-accent-2)] leading-tight">{t("comp.asrSwitched", { lang: asrNote })}</p>
            : <p className="mt-1 text-center text-[10px] text-[var(--color-muted)] leading-tight">{t("comp.langHint")}</p>}
          <div className="mt-3 flex flex-col items-center gap-1">
            <label className="flex items-center gap-2 text-[12px] text-[var(--color-muted)]">
              {t("speakers.count")}
              <select value={speakerCount} onChange={(e) => chooseSpeakerCount(Number(e.target.value))}
                className="rounded-md border border-[var(--color-border)] bg-[var(--color-surface-2)] px-2 py-1 text-[var(--color-text)]">
                <option value={0}>{t("settings.auto")}</option>
                {Array.from({ length: 8 }, (_, i) => i + 1).map((n) => <option key={n} value={n}>{new Intl.NumberFormat(i18n.language).format(n)}</option>)}
              </select>
            </label>
            <p className="max-w-sm text-center text-[10px] text-[var(--color-muted)] leading-tight">{t("speakers.countHint")}</p>
          </div>

          {/* Импорт готовых субтитров (SRT/ASS): точный текст+тайминг вместо авто-распознавания (ASR skip). */}
          {(() => {
            const subsChoice = (
              <>
                <label className="inline-flex items-center gap-1.5 text-[10px] text-[var(--color-muted)] cursor-pointer" title={t("import.translatedHint")}>
                  <input type="checkbox" checked={subsTranslated} onChange={(e) => setSubsTranslated(e.target.checked)} className="accent-[var(--color-accent)]" />
                  {t("import.translated")}
                </label>
                {!subsTranslated && <SubsAlignToggle checked={subsAlign} onChange={setSubsAlign} />}
              </>
            );
            // Субтитры площадки уже лежат в скачанном проекте: выбрать другой файл к нему нельзя, только как их брать.
            if (fetched) {
              return hasSubs && (
                <div className="mt-2 flex flex-col items-center gap-1">
                  <span className="inline-flex items-center gap-1.5 text-[11px] text-[var(--color-accent-2)]"><Captions size={13} />{t("url.subs")} · <span className="mono">{fetched.subsLang}</span></span>
                  {subsChoice}
                </div>
              );
            }
            return !audioOnly && (
              <div className="mt-2 flex flex-col items-center gap-0.5">
                <label className={`inline-flex items-center gap-1.5 text-[11px] cursor-pointer transition-colors ${subsFile ? "text-[var(--color-accent-2)]" : "text-[var(--color-muted)] hover:text-[var(--color-accent-2)]"}`}>
                  <Captions size={13} />
                  {subsFile ? subsFile.name : t("import.subs")}
                  <input type="file" accept=".srt,.ass,.ssa" className="hidden"
                    onChange={(e) => { const f = e.target.files?.[0] ?? null; setSubsFile(f); e.currentTarget.value = ""; }} />
                </label>
                {subsFile
                  ? <div className="flex flex-col items-center gap-1 mt-0.5">
                      {subsChoice}
                      <button onClick={() => { setSubsFile(null); setSubsTranslated(false); setSubsAlign(false); }} className="inline-flex items-center gap-1 text-[10px] text-[var(--color-muted)] hover:text-[var(--color-text)]"><X size={10} />{t("import.subsClear")}</button>
                    </div>
                  : <span className="text-[10px] text-[var(--color-muted)] leading-tight text-center max-w-[300px]">{t("import.subsHint")}</span>}
              </div>
            );
          })()}
          {/* АУДИО-ВЫХОД (независимо от субтитров) */}
          <div className="mt-3 flex items-center gap-1.5 text-[11px] uppercase tracking-[0.1em] text-[var(--color-muted)] mb-1">{t("comp.audioLabel")}<span title={t("comp.optionsHelp")} className="cursor-help inline-flex text-[var(--color-muted)] opacity-40 hover:opacity-100 hover:text-[var(--color-accent-2)] transition"><HelpCircle size={12} /></span></div>
          <div className="grid grid-cols-2 gap-1.5">
            {([["nodub", Captions, "comp.audioNone", "comp.audioNoneHint"], ["dub", AudioLines, "comp.audioDub", "comp.audioDubHint"], ["voiceover", Mic2, "comp.audioVoiceover", "comp.audioVoiceoverHint"], ["transcribe", FileText, "mode.transcribe", "comp.audioTranscribeHint"]] as const).map(([a, Icon, key, hint]) => (
              <button key={a} onClick={() => chooseAudio(a)} title={t(hint)}
                className={`flex items-center justify-center gap-1.5 px-2 py-2 rounded-lg border text-[12px] font-medium transition-colors ${audio === a ? "border-[var(--color-accent)] bg-[color-mix(in_oklab,var(--color-accent)_12%,transparent)] text-[var(--color-text)]" : "border-[var(--color-border)] bg-[var(--color-surface-2)] text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
                <Icon size={14} />{t(key)}</button>
            ))}
          </div>
          {/* Прогрессивное раскрытие (#118): вторичные опции — в свёрнутых аккордеонах, только релевантные режиму.
              Все стейты/обработчики/условия те же, изменилась лишь группировка разметки. */}
          {(() => {
            const isVoiced = audio === "dub" || audio === "voiceover";
            const showSubs = audio !== "transcribe" && !audioOnly;
            // Статусы для свёрнутых заголовков.
            const voicesStatus = voiceSrc === "clone"
              ? t("voiceSlots.clone")
              : t("accordion.voicesLib", { count: slotsM.length + slotsF.length });
            const subsStatus = t(subs === "none" ? "comp.subsNone" : subs === "transcribe" ? "comp.subsOriginal" : subs === "bilingual" ? "comp.subsBilingual" : "comp.subsTranslate");
            // «Дополнительно» показываем только если при текущем режиме внутри есть хоть один пункт.
            // стиль перевода — только там, где перевод реально идёт (не в транскрипт-режиме).
            const showTrStyle = audio === "dub" || audio === "voiceover" || ((subs === "translate" || subs === "bilingual") && audio !== "transcribe");
            const showKeepOrig = isVoiced && !audioOnly;
            const showDetect = showSubs;
            const showCasting = !audioOnly && isVoiced;   // #115: кастинг имеет смысл только при дубляже видео
            const showFunny = isVoiced;
            const showVoGain = audio === "voiceover";
            const showDuck = audio === "dub" && !audioOnly;   // дакинг фона — опция дубляжа
            const showBlur = showSubs && subs !== "none" && burn;   // блюр-подложка — только когда сабы реально вжигаются
            const showAdvanced = showTrStyle || showKeepOrig || showDetect || showCasting || showFunny || showVoGain || showDuck || showBlur;
            return (
              <>
                {/* ГОЛОСА (#114): клон из видео (дефолт) или слоты из библиотеки. */}
                {isVoiced && (
                  <Accordion title={t("accordion.voices")} subtitle={voicesStatus}>
                    <div className="grid grid-cols-2 gap-1.5">
                      {(["clone", "library"] as const).map((m) => (
                        <button key={m} onClick={() => setVoiceSrcSaved(m)}
                          className={`px-2 py-1.5 rounded-lg border text-[11px] font-medium transition-colors ${voiceSrc === m ? "border-[var(--color-accent)] bg-[color-mix(in_oklab,var(--color-accent)_12%,transparent)] text-[var(--color-text)]" : "border-[var(--color-border)] bg-[var(--color-surface-2)] text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
                          {t(m === "clone" ? "voiceSlots.clone" : "voiceSlots.library")}</button>
                      ))}
                    </div>
                    {voiceSrc === "library" && (
                      <div className="mt-1.5 space-y-2">
                        <p className="text-[10px] text-[var(--color-muted)] leading-tight">{t("voiceSlots.priorityHint")}</p>
                        <VoiceSlotList title={t("voiceSlots.male")} voices={voiceLib} slots={slotsM} onChange={setSlotsMSaved} />
                        <VoiceSlotList title={t("voiceSlots.female")} voices={voiceLib} slots={slotsF} onChange={setSlotsFSaved} />
                      </div>
                    )}
                  </Accordion>
                )}
                {/* СУБТИТРЫ (независимо от аудио) */}
                {showSubs && (
                  <Accordion title={t("accordion.subs")} subtitle={subsStatus}>
                    <div className="grid grid-cols-2 gap-1.5">
                      {([["none", "comp.subsNone", "comp.subsNoneHint"], ["transcribe", "comp.subsOriginal", "comp.subsOriginalHint"], ["translate", "comp.subsTranslate", "comp.subsTranslateHint"], ["bilingual", "comp.subsBilingual", "comp.subsBilingualHint"]] as const).map(([sv, key, hint]) => (
                        <button key={sv} onClick={() => chooseSubs(sv)} title={t(hint)}
                          className={`px-2 py-1.5 rounded-lg border text-[11px] font-medium transition-colors ${subs === sv ? "border-[var(--color-accent)] bg-[color-mix(in_oklab,var(--color-accent)_12%,transparent)] text-[var(--color-text)]" : "border-[var(--color-border)] bg-[var(--color-surface-2)] text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
                          {t(key)}</button>
                      ))}
                    </div>
                    {subs !== "none" && (
                      <label className="mt-1.5 flex items-center gap-2 text-[12px] cursor-pointer select-none w-fit">
                        <input type="checkbox" checked={burn} onChange={(e) => chooseBurn(e.target.checked)} className="accent-[var(--color-accent)] w-3.5 h-3.5" />
                        {t("comp.burn")}
                        <span title={t("comp.burnHint")} onClick={(e) => e.preventDefault()} className="cursor-help inline-flex text-[var(--color-muted)] opacity-40 hover:opacity-100 hover:text-[var(--color-accent-2)] transition"><HelpCircle size={12} /></span>
                      </label>
                    )}
                  </Accordion>
                )}
                {/* ДОПОЛНИТЕЛЬНО: стиль перевода / оригинал-дорожка / детекция текста / шуточный ремикс / громкость оригинала. */}
                {showAdvanced && (
                  <Accordion title={t("accordion.advanced")}>
                    {/* СТИЛЬ ПЕРЕВОДА (#112): доп-регистр для Геммы. */}
                    {showTrStyle && (
                      <div>
                        <div className="flex items-center gap-1.5 text-[11px] uppercase tracking-[0.1em] text-[var(--color-muted)] mb-1">{t("trStyle.label")}</div>
                        <select value={trStyle} onChange={(e) => { const v = TR_STYLE_IDS.find((id) => id === e.target.value); if (v !== undefined) setTrStyleSaved(v); }}
                          className="w-full bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-lg px-2.5 py-1.5 text-[12px] focus:border-[var(--color-accent)] focus:outline-none">
                          <option value="">{t("trStyle.normal")}</option>
                          <option value="technical">{t("trStyle.technical")}</option>
                          <option value="literary">{t("trStyle.literary")}</option>
                          <option value="casual">{t("trStyle.casual")}</option>
                          <option value="custom">{t("trStyle.custom")}</option>
                        </select>
                        {trStyle === "custom" && (
                          <textarea value={trStyleCustom} onChange={(e) => setTrStyleCustomSaved(e.target.value)} rows={2} placeholder={t("trStyle.customPlaceholder")}
                            className="mt-1.5 w-full bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-lg px-2.5 py-2 text-[12px] focus:border-[var(--color-accent)] focus:outline-none resize-none" />
                        )}
                      </div>
                    )}
                    {/* СОХРАНИТЬ ОРИГИНАЛ (#113): 2-я аудиодорожка в выводе + контейнер. */}
                    {showKeepOrig && (
                      <div className={showTrStyle ? "mt-2.5" : ""}>
                        <label className="flex items-center gap-2 text-[12px] cursor-pointer select-none w-fit" title={t("keepOrig.hint")}>
                          <input type="checkbox" checked={keepOrig} onChange={(e) => setKeepOrigSaved(e.target.checked)} className="accent-[var(--color-accent)] w-3.5 h-3.5" />
                          {t("keepOrig.label")}
                          <span title={t("keepOrig.hint")} onClick={(e) => e.preventDefault()} className="cursor-help inline-flex text-[var(--color-muted)] opacity-40 hover:opacity-100 hover:text-[var(--color-accent-2)] transition"><HelpCircle size={12} /></span>
                        </label>
                        {keepOrig && (
                          <div className="mt-1.5 flex rounded-lg border border-[var(--color-border)] overflow-hidden text-[11px] w-fit">
                            {(["mp4", "mkv"] as const).map((c) => (
                              <button key={c} onClick={() => setContainerSaved(c)} title={t(c === "mp4" ? "keepOrig.mp4Hint" : "keepOrig.mkvHint")}
                                className={`px-2.5 py-1 transition-colors ${container === c ? "bg-[var(--color-accent)] text-[var(--color-on-accent)]" : "text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
                                {t(c === "mp4" ? "keepOrig.mp4" : "keepOrig.mkv")}</button>
                            ))}
                          </div>
                        )}
                      </div>
                    )}
                    {/* ДАКИНГ ФОНА ПОД ДУБЛЯЖОМ — опция дубляжа (не всем нужен), ВЫКЛ по умолч.: вкл = фон
                        тише под голосом, выкл = фон на полной громкости. */}
                    {showDuck && (
                      <label className="mt-1.5 flex items-center gap-2 text-[12px] cursor-pointer select-none w-fit" title={t("duck.tip")}>
                        <input type="checkbox" checked={duckOn} onChange={(e) => setDuckSaved(e.target.checked)} className="accent-[var(--color-accent)] w-3.5 h-3.5" />
                        {t("duck.label")}
                        <span title={t("duck.hint")} onClick={(e) => e.preventDefault()} className="cursor-help inline-flex text-[var(--color-muted)] opacity-40 hover:opacity-100 hover:text-[var(--color-accent-2)] transition"><HelpCircle size={12} /></span>
                      </label>
                    )}
                    {/* ДЕТЕКЦИЯ ВШИТОГО ТЕКСТА (OCR). */}
                    {showDetect && (
                      <label className="mt-1.5 flex items-center gap-2 text-[12px] cursor-pointer select-none w-fit">
                        <input type="checkbox" checked={detectText} onChange={(e) => chooseDetect(e.target.checked)} className="accent-[var(--color-accent)] w-3.5 h-3.5" />
                        {t("comp.detect")}
                        <span title={t("comp.detectHint")} onClick={(e) => e.preventDefault()} className="cursor-help inline-flex text-[var(--color-muted)] opacity-40 hover:opacity-100 hover:text-[var(--color-accent-2)] transition"><HelpCircle size={12} /></span>
                      </label>
                    )}
                    {/* БЛЮР-ПОДЛОЖКА ПОД СУБТИТРАМИ — опция (не всем нужна): выкл = текст без размытой подложки. */}
                    {showBlur && (
                      <label className="mt-1.5 flex items-center gap-2 text-[12px] cursor-pointer select-none w-fit" title={t("blurPlate.tip")}>
                        <input type="checkbox" checked={subBlur} onChange={(e) => setSubBlurSaved(e.target.checked)} className="accent-[var(--color-accent)] w-3.5 h-3.5" />
                        {t("blurPlate.label")}
                        <span title={t("blurPlate.hint")} onClick={(e) => e.preventDefault()} className="cursor-help inline-flex text-[var(--color-muted)] opacity-40 hover:opacity-100 hover:text-[var(--color-accent-2)] transition"><HelpCircle size={12} /></span>
                      </label>
                    )}
                    {/* КАСТИНГ ПЕРСОНАЖЕЙ (#115): доп. проход по кадрам -> база персонажей (аватар/голос). Опц., дефолт ВЫКЛ. */}
                    {showCasting && (
                      <label className="mt-1.5 flex items-center gap-2 text-[12px] cursor-pointer select-none w-fit">
                        <input type="checkbox" checked={castingOn} onChange={(e) => setCastingSaved(e.target.checked)} className="accent-[var(--color-accent)] w-3.5 h-3.5" />
                        {t("comp.casting")}
                        <span title={t("comp.castingHint")} onClick={(e) => e.preventDefault()} className="cursor-help inline-flex text-[var(--color-muted)] opacity-40 hover:opacity-100 hover:text-[var(--color-accent-2)] transition"><HelpCircle size={12} /></span>
                      </label>
                    )}
                    {/* ТИП КОНТЕНТА (#115): авто (Gemma-детект) | реальные лица (SCRFD+LVFace) | мультфильм/аниме
                        (детектор рисованных лиц + CCIP). Виден только при вкл. кастинге. Сегмент-контрол из трёх кнопок. */}
                    {showCasting && castingOn && (
                      <div className="mt-1.5 inline-flex rounded-lg border border-[var(--color-border)] overflow-hidden text-[11px] w-fit">
                        {([["auto", t("casting.contentAuto")], ["real", t("casting.contentReal")], ["anime", t("casting.contentAnime")]] as const).map(([v, lbl]) => (
                          <button key={v} type="button" onClick={() => setContentTypeSaved(v)}
                            className={`px-2.5 py-1 transition ${contentType === v ? "bg-[var(--color-accent)] text-[var(--color-on-accent)]" : "text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
                            {lbl}
                          </button>
                        ))}
                      </div>
                    )}
                    {/* ПРИМЕНИТЬ ГОТОВЫЙ КАСТИНГ (#115): профиль из библиотеки -> analyze(casting_ref=). Виден только при вкл. кастинге. */}
                    {showCasting && castingOn && (
                      <div className="mt-1.5">
                        <div className="flex items-center gap-1.5 text-[11px] uppercase tracking-[0.1em] text-[var(--color-muted)] mb-1">{t("castingLib.applyLabel")}</div>
                        <div className="flex items-center gap-1.5">
                          <select value={castLib.some((c) => c.slug === castingRef) ? castingRef : ""} onChange={(e) => setCastingRefSaved(e.target.value)}
                            className="flex-1 min-w-0 bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-lg px-2.5 py-1.5 text-[12px] focus:border-[var(--color-accent)] focus:outline-none">
                            <option value="">{t("castingLib.none")}</option>
                            {castLib.map((c) => (
                              <option key={c.slug} value={c.slug}>{t("castingLib.profileOption", { name: c.name, n: c.char_count })}</option>
                            ))}
                          </select>
                          {castingRef && castLib.some((c) => c.slug === castingRef) && (
                            <button type="button" title={t("castingLib.delete")}
                              onClick={() => setDeletingCast(castingRef)}
                              className="shrink-0 p-1.5 rounded-lg border border-[var(--color-border)] text-[var(--color-muted)] hover:text-[var(--color-danger,#ef4444)] hover:border-[var(--color-danger,#ef4444)] transition">
                              <Trash2 size={13} />
                            </button>
                          )}
                        </div>
                        {!castLib.length && <p className="mt-1 text-[10px] text-[var(--color-muted)] leading-tight">{t("castingLib.empty")}</p>}
                      </div>
                    )}
                    {/* ШУТОЧНЫЙ РЕМИКС — сочетается с дубляжом/закадром. */}
                    {showFunny && (
                      <>
                        <label className="mt-2 flex items-center gap-2 text-[12px] cursor-pointer select-none">
                          <input type="checkbox" checked={funnyOn} onChange={(e) => setFunnyOn(e.target.checked)} className="accent-[var(--color-accent)] w-3.5 h-3.5" />
                          <Sparkles size={13} className="text-[var(--color-accent-2)]" />{t("comp.funny")}
                        </label>
                        {funnyOn && (
                          <textarea value={funny} onChange={(e) => setFunny(e.target.value)} rows={2} placeholder={t("remix.placeholder")}
                            className="mt-1.5 w-full bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-lg px-2.5 py-2 text-[12px] focus:border-[var(--color-accent)] focus:outline-none resize-none" />
                        )}
                      </>
                    )}
                    {/* ЗАКАДРОВЫЙ: громкость оригинала под переводом — глобальный дефолт. */}
                    {showVoGain && (
                      <div className="mt-2">
                        <div className="flex items-center justify-between text-[11px] mb-1">
                          <span className="text-[var(--color-muted)]">{t("comp.origGain")}</span>
                          <span className="mono text-[11px] text-[var(--color-text)]">{voGain.toFixed(1)} dB</span>
                        </div>
                        <input type="range" min={-24} max={0} step={0.5} value={voGain}
                          onChange={(e) => setVoGainSaved(parseFloat(e.target.value))}
                          className="w-full accent-[var(--color-accent)]" />
                      </div>
                    )}
                  </Accordion>
                )}
              </>
            );
          })()}
          <div className="mt-2.5 flex items-center gap-2">
            <button onClick={run} disabled={(!file && !fetched) || (funnyOn && (audio === "dub" || audio === "voiceover") && !funny.trim())}
              className="flex-1 inline-flex items-center justify-center gap-2 px-4 py-2.5 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-sm font-semibold disabled:opacity-40 hover:brightness-105 transition">
              {t("drop.start")} <ArrowRight size={16} />
            </button>
            <button onClick={runManual} disabled={!file && !fetched} title={t("manual.tip")}
              className="inline-flex flex-col items-center justify-center px-3 py-1.5 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-2)] text-[var(--color-text)] hover:border-[var(--color-accent)] hover:text-[var(--color-accent)] disabled:opacity-40 transition-colors">
              <span className="text-[12px] font-semibold flex items-center gap-1"><Sliders size={13} /> {t("manual.label")}</span>
              <span className="text-[9px] text-[var(--color-muted)] font-normal leading-none mt-0.5">{t("manual.sub")}</span>
            </button>
          </div>
          <button onClick={() => batchRef.current?.click()}
            className="mt-2 w-full inline-flex items-center justify-center gap-2 px-4 py-2 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-2)] text-[var(--color-muted)] text-[13px] font-medium hover:text-[var(--color-text)] hover:border-[#3a414c] transition">
            <FolderDown size={15} />{t("batch.open")}</button>
          <div className="mt-2 flex items-center justify-center gap-2 text-[12px] text-[var(--color-muted)]">
            <ShieldCheck size={14} className="text-[var(--color-accent-2)]" /> {t("hero.formats")}
          </div>
        </motion.div>
      </motion.div>
      {allProjects && (
        <ProjectsList projects={recent} onOpen={(pid) => { setAllProjects(false); void openProject(pid); }}
          onDelete={deleteProject} onClose={() => setAllProjects(false)} />
      )}
      {deleting && (
        <ConfirmDialog danger title={t("recent.delete")} message={t("recent.deleteConfirm", { video: deleting.video })} confirmLabel={t("projects.deleteConfirm")}
          onCancel={() => setDeleting(null)}
          onConfirm={async () => { await deleteProject(deleting.pid); setDeleting(null); }} />
      )}
      {deletingCast && (
        <ConfirmDialog danger title={t("castingLib.delete")} message={t("castingLib.deleteConfirm")} confirmLabel={t("castingLib.delete")}
          onCancel={() => setDeletingCast(null)}
          onConfirm={async () => {
            await api.deleteCastingLibrary(deletingCast);
            if (castingRef === deletingCast) setCastingRefSaved("");
            setDeletingCast(null);
            refreshCastLib();
          }} />
      )}
      <input ref={inputRef} type="file" accept={MEDIA_ACCEPT} className="hidden"
        onChange={(e) => { const f = e.target.files?.[0]; if (f) pickFile(f); }} />
      <input ref={batchRef} type="file" multiple accept={MEDIA_ACCEPT} className="hidden"
        onChange={(e) => { const fs = [...(e.target.files || [])]; if (fs.length) { batchState.files = fs; batchState.tgt = tgt; batchState.src = src; batchState.audio = audio; batchState.subs = subs; batchState.burn = burn; batchState.detectText = detectText; batchState.funnyOn = funnyOn; batchState.funny = funny; batchState.voGain = voGain; batchState.trStyle = resolveTrStyle(trStyle, trStyleCustom); batchState.keepOrig = keepOrig; batchState.container = container; batchState.voiceSrc = voiceSrc; batchState.slotsM = slotsM; batchState.slotsF = slotsF; s.setStage("batch"); } }} />
    </div>
  );
}

// `allowed` — ключи шагов ТЕКУЩЕЙ джобы: стадия отфильтрованного шага (напр. ocr_detect при
// выключенной детекции) не должна подписываться его ярлыком в статус-строке — вернём null, и
// строка покажет сырое сообщение бэкенда («детекция вшитого текста отключена»), а не фантомный шаг.
function stageLabel(stage: string | undefined, t: TFunction<"t">, allowed?: string[] | null): string | null {
  if (!stage) return null;
  const k = STAGE_TO_STEPKEY[stage];
  if (!k) return null;
  if (allowed && !allowed.includes(k)) return null;
  return t(`analyze.${k}`);
}

function AnalyzeProgress() {
  const { t } = useTranslation();
  const { progress, audioOnly, jobSteps, resumedStages, queuedAhead } = useStore();
  // Показываем только шаги текущей джобы (jobSteps из run()); null (открытие по ?pid и т.п.) = все.
  const STEPS = jobSteps ? ANALYZE_STEPS.filter((stp) => jobSteps.includes(stp.key)) : ANALYZE_STEPS;
  const matched = STEPS.findIndex((stp) => stp.stages.includes(progress.stage));
  // монотонно: неизвестная/незамапленная стадия (напр. промежуточный лог) НЕ гасит прогресс — держим
  // самый дальний достигнутый шаг. Компонент перемонтируется на каждый прогон, поэтому ref сбрасывается.
  const maxStep = useRef(-1);
  if (matched > maxStep.current) maxStep.current = matched;
  const cur = maxStep.current;
  const dl = progress.stage === "download";
  const pct = progress.pct;
  return (
    <div className="flex-1 grid place-items-center px-6">
      <div className="w-full max-w-sm">
        <div className="text-center text-lg font-semibold">{audioOnly ? t("analyze.titleAudio") : t("analyze.title")}</div>
        {dl && <div className="mt-1 text-center text-[12px] text-[var(--color-muted)]">{t("analyze.firstRunNote")}</div>}
        <div className="mt-6 space-y-2.5">
          {STEPS.map((stp, i) => {
            const done = cur > i, active = cur === i;
            return (
              <div key={stp.key} className="flex items-center gap-3">
                <span className={`grid place-items-center w-5 h-5 shrink-0 rounded-full ${done ? "bg-[var(--color-accent)] text-[var(--color-on-accent)]" : active ? "text-[var(--color-accent)]" : "text-[var(--color-muted)]/40"}`}>
                  {done ? <Check size={12} /> : active ? <Loader2 size={14} className="animate-spin" /> : <span className="w-1.5 h-1.5 rounded-full bg-current" />}
                </span>
                <span className={`text-sm ${active ? "text-[var(--color-text)] font-medium" : done ? "text-[var(--color-muted)]" : "text-[var(--color-muted)]/45"}`}>{t(`analyze.${stp.key}`)}</span>
                {stp.stages.some((st) => resumedStages.includes(st)) && <span className="ml-auto text-[10px] uppercase tracking-wider text-[var(--color-accent-2)]">{t("jobs.fromCache")}</span>}
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
        <div className="mt-2 min-h-4 text-center mono text-[12px] text-[var(--color-muted)] break-words">{queuedAhead != null ? t("jobs.queuedAhead", { n: queuedAhead }) : stageLabel(progress.stage, t, jobSteps) || progress.msg}</div>
        <JobFailurePanel />
        <CancelJobButton />
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
    <button onClick={onClick} role="switch" aria-checked={on} aria-label={label || undefined} className="flex items-center justify-between w-full">
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

function WaveformTimeline({ pid, duration, scrub, segments, onSeek, gainDb = 0 }: {
  pid: string; duration: number; scrub: number; segments: Project["segments"]; onSeek: (t: number) => void; gainDb?: number;
}) {
  const [peaks, setPeaks] = useState<number[]>([]);
  const wrap = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(800);
  useEffect(() => { api.waveform(pid).then((r) => setPeaks(r.peaks)).catch(() => {}); }, [pid]);
  useEffect(() => { const el = wrap.current; if (!el) return; const ro = new ResizeObserver(() => setW(el.clientWidth)); ro.observe(el); return () => ro.disconnect(); }, []);
  const h = 40, dur = duration || 1, bw = peaks.length ? w / peaks.length : 1;
  const gainLin = Math.pow(10, gainDb / 20);   // dB -> линейный коэффициент амплитуды (гейн дорожки визуально)
  return (
    <div ref={wrap} className="relative w-full overflow-hidden cursor-pointer select-none" style={{ height: h }}
      onClick={(e) => { const r = e.currentTarget.getBoundingClientRect(); onSeek(Math.max(0, Math.min(dur, (e.clientX - r.left) / r.width * dur))); }}>
      <svg width="100%" height={h} viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" className="block">
        {peaks.map((pk, i) => {
          const bh = Math.max(2, Math.min(h - 2, pk * gainLin * (h - 6))), played = (i / peaks.length) * dur <= scrub;
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
  const { t } = useTranslation();
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
      <div role="dialog" aria-modal="true" aria-label={t("palette.placeholder")} className="w-[min(92vw,520px)] rounded-xl glass-panel anim-pop overflow-hidden" onClick={(e) => e.stopPropagation()}>
        <input autoFocus value={q} onChange={(e) => setQ(e.target.value)} placeholder={t("palette.placeholder")} aria-label={t("palette.placeholder")}
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

// Фокус в поле ввода -> хоткеи не перехватываем (образец гарда — как в undo/redo). Плюс сюда попадает
// contentEditable (например, комбобоксы). Общий для всех медиа-биндов.
function inTextField(e: KeyboardEvent): boolean {
  const el = e.target as HTMLElement | null;
  if (!el) return false;
  const tag = el.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || el.isContentEditable;
}

// Запросить фулскрин для элемента через БРАУЗЕРНЫЙ Fullscreen API (НЕ @tauri-apps/api: фронт грузится с
// внешнего http://127.0.0.1 — JS-IPC ненадёжен). Esc выходит нативно. Тумблер: если уже фулскрин — выйти.
function toggleElemFullscreen(el: Element | null) {
  if (!el) return;
  if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
  else (el as HTMLElement).requestFullscreen?.().catch(() => {});
}

// Единый набор медиа-хоткеев для превью (Editor + TranscriptView). Space/K — плей/пауза; ←/→ ±5с,
// Shift ±1с, Ctrl ±10с; Home/End; ↑/↓ громкость (если задан setVol); F — фулскрин области превью;
// F10 — фулскрин окна; ? — шпаргалка. Ctrl+колесо над превью — перемотка ~1с/тик (passive:false +
// preventDefault, иначе WebView2 зумит). Гард: не перехватываем в полях ввода и при открытой модалке.
function useMediaHotkeys(opts: {
  enabled: boolean;
  duration: number;
  scrubRef: React.MutableRefObject<number>;
  seek: (t: number) => void;
  togglePlay: () => void;
  previewRef: React.RefObject<HTMLElement | null>;
  setHelp: (v: boolean) => void;
  blocked?: () => boolean;   // напр. открыт CommandPalette/модалка -> не перехватывать
  vol?: number;
  setVol?: (v: number) => void;
}) {
  const { enabled, duration, scrubRef, seek, togglePlay, previewRef, setHelp, blocked, vol, setVol } = opts;
  useEffect(() => {
    if (!enabled) return;
    const clamp = (t: number) => Math.max(0, Math.min(duration || 0, t));
    const onKey = (e: KeyboardEvent) => {
      if (inTextField(e) || e.altKey || e.metaKey) return;
      // Ctrl пропускаем ТОЛЬКО для ←/→ (перемотка ±10с). Иначе Ctrl+K/Ctrl+Z (палитра/undo) остаются за своими хендлерами.
      if (e.ctrlKey && e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
      if (blocked?.()) return;
      const cur = scrubRef.current;
      // Space на сфокусированной кнопке/ссылке — это её нативная активация, свой хоткей тут не навешиваем
      // (иначе двойной тогл). K работает всегда.
      const onButton = (() => { const a = document.activeElement as HTMLElement | null; return !!a && (a.tagName === "BUTTON" || a.tagName === "A" || a.getAttribute("role") === "button"); })();
      switch (e.key) {
        case " ":
          if (onButton) return;
          e.preventDefault(); togglePlay(); break;
        case "k": case "K":
          e.preventDefault(); togglePlay(); break;
        case "ArrowRight":
          e.preventDefault(); seek(clamp(cur + (e.shiftKey ? 1 : e.ctrlKey ? 10 : 5))); break;
        case "ArrowLeft":
          e.preventDefault(); seek(clamp(cur - (e.shiftKey ? 1 : e.ctrlKey ? 10 : 5))); break;
        case "Home":
          e.preventDefault(); seek(0); break;
        case "End":
          e.preventDefault(); seek(clamp(duration)); break;
        case "ArrowUp":
        case "ArrowDown": {
          // ↑/↓ = громкость, но НЕ когда фокус в скроллируемом списке — там стрелки должны прокручивать.
          const el = e.target as HTMLElement;
          if (el.closest("[data-kb-scroll]")) return;
          if (setVol) { e.preventDefault(); setVol(Math.max(0, Math.min(1, (vol ?? 1) + (e.key === "ArrowUp" ? 0.05 : -0.05)))); }
          break;
        }
        case "f": case "F":
          e.preventDefault(); toggleElemFullscreen(previewRef.current); break;
        case "F10":
          e.preventDefault();
          if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
          else document.documentElement.requestFullscreen?.().catch(() => {});
          break;
        case "?":
          e.preventDefault(); setHelp(true); break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [enabled, duration, seek, togglePlay, previewRef, setHelp, blocked, vol, setVol, scrubRef]);

  // Ctrl+колесо над контейнером превью -> перемотка (шаг ~1с/тик). passive:false обязателен для preventDefault.
  useEffect(() => {
    if (!enabled) return;
    const el = previewRef.current; if (!el) return;
    const clamp = (t: number) => Math.max(0, Math.min(duration || 0, t));
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey) return;                 // обычное колесо не трогаем (пусть скроллит)
      e.preventDefault();                     // иначе WebView2 зумит страницу
      seek(clamp(scrubRef.current + (e.deltaY > 0 ? 1 : -1)));
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [enabled, duration, seek, previewRef, scrubRef]);
}

// Оверлей-шпаргалка горячих клавиш (стиль CommandPalette: glass-scrim/glass-panel). Таблица «клавиша —
// действие» из i18n. Esc/клик мимо закрывает. Открывается по ? и из палитры команд.
function ShortcutsHelp({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  useEffect(() => {
    const h = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", h); return () => window.removeEventListener("keydown", h);
  }, [onClose]);
  const rows: [string, string][] = [
    ["Space · K", t("hotkeys.playPause")],
    ["← / →", t("hotkeys.seek5")],
    ["Shift + ← / →", t("hotkeys.seek1")],
    ["Ctrl + ← / →", t("hotkeys.seek10")],
    ["Home · End", t("hotkeys.homeEnd")],
    ["↑ / ↓", t("hotkeys.volume")],
    ["F", t("hotkeys.fsPreview")],
    ["F10", t("hotkeys.fsWindow")],
    ["Ctrl + " + "\u{1F5B1}", t("hotkeys.wheelSeek")],
    ["?", t("hotkeys.help")],
    ["Esc", t("hotkeys.esc")],
  ];
  return (
    <div className="fixed inset-0 z-[60] grid place-items-center glass-scrim anim-fade" onClick={onClose}>
      <div role="dialog" aria-modal="true" aria-labelledby="hotkeys-title" className="w-[min(92vw,480px)] rounded-xl glass-panel anim-pop overflow-hidden" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center gap-2 px-4 py-3 border-b border-[var(--color-border)]">
          <Keyboard size={16} className="text-[var(--color-accent)]" />
          <span id="hotkeys-title" className="text-[14px] font-semibold">{t("hotkeys.title")}</span>
          <button onClick={onClose} {...named(t("a11y.close"))} className="ml-auto text-[var(--color-muted)] hover:text-[var(--color-text)]"><X size={16} /></button>
        </div>
        <div className="p-2">
          {rows.map(([key, act]) => (
            <div key={key} className="flex items-center justify-between px-3 py-1.5 rounded-lg hover:bg-[var(--color-surface-2)] transition-colors">
              <span className="text-[13px] text-[var(--color-text)]">{act}</span>
              <kbd className="mono text-[11px] px-2 py-0.5 rounded bg-[var(--color-surface-2)] border border-[var(--color-border)] text-[var(--color-muted)] shrink-0 ml-4">{key}</kbd>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

// Textarea фразы с АВТО-высотой под объём текста (реквест юзера: фикс-2-строки обрезали длинные
// переводы, работать неудобно). height=scrollHeight на каждом изменении значения и на маунте.
function AutoGrowTextarea(props: React.TextareaHTMLAttributes<HTMLTextAreaElement>) {
  const ref = useRef<HTMLTextAreaElement>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";                       // сброс, чтобы уметь и уменьшаться
    el.style.height = `${el.scrollHeight + 2}px`;   // +2 — бордеры, иначе появляется скролл на 1px
  }, [props.value]);
  return <textarea ref={ref} rows={1} {...props} />;
}

// Кастинг персонажей (#115): Кинопоиск-style сетка карточек. Аватар (кадр из видео) сверху, поля снизу:
// имя, пол (бейдж), «характер речи» (уходит в Гемму как speech_note), голос дубляжа (селект из библиотеки),
// счётчик реплик. Правки локальные -> «Применить кастинг» POST /casting. База актёров переживает язык/проект.
function CastingPanel({ pid, characters, voices, onChange }: {
  pid: string; characters: Character[]; voices: string[]; onChange: (c: Character[]) => void;
}) {
  const { t } = useTranslation();
  // Черновик правок: имя / заметка о речи / голос — редактируются локально, коммит одной кнопкой.
  const [draft, setDraft] = useState<Record<string, { name: string; speech_note: string; voice: string | null }>>(
    () => Object.fromEntries(characters.map((c) => [c.id, { name: c.name, speech_note: c.speech_note ?? "", voice: c.voice }])),
  );
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  // Облачный TTS (OpenRouter): в кастинге выбираем ОБЛАЧНЫЕ голоса модели, а не локальную библиотеку клонов.
  // Пусто у персонажа -> автокастинг по полу на рендере. Флаг + список тянем из настроек/каталога модели.
  const [cloudOn, setCloudOn] = useState(false);
  const [cloudVoices, setCloudVoices] = useState<{ name: string; gender: string }[]>([]);
  useEffect(() => {
    api.capabilities().then((cap) => {
      const provider = slot(cap.selection,"tts_provider") ?? ((cap.selection?.or_tts_on ?? "") === "1" ? "openrouter" : "local");
      const on = provider !== "local";
      setCloudOn(on);
      const model = slot(cap.selection, provider === "google" ? "google_tts_model" : "or_tts_model") ?? "";
      if (on && model) api.openrouterVoices(model).then((r) => setCloudVoices(r.voices.map((v) => ({ name: v.name, gender: v.gender })))).catch(() => {});
    }).catch(() => {});
  }, []);
  // Аватарки без кадра (sample_frame_url=null, напр. закадровый) или с битой ссылкой -> заглушка-инициал.
  const [imgFail, setImgFail] = useState<Record<string, boolean>>({});
  // #115/#6: characters могут обновиться извне (после apply() cross-episode-матчинг возвращает ДРУГИЕ id;
  // либо ресинк из стора). Ре-мёржим черновик: правки по существующим id сохраняем, новые id добавляем из c.
  // imgFail чистим только для исчезнувших id (для новых стартует false -> покажем аватар).
  useEffect(() => {
    setDraft((d) => {
      const next: typeof d = {};
      for (const c of characters) next[c.id] = d[c.id] ?? { name: c.name, speech_note: c.speech_note ?? "", voice: c.voice };
      return next;
    });
    setImgFail((m) => {
      const next: typeof m = {};
      for (const c of characters) if (m[c.id]) next[c.id] = true;
      return next;
    });
  }, [characters]);
  const setField = (id: string, patch: Partial<{ name: string; speech_note: string; voice: string | null }>) => {
    setDraft((d) => ({ ...d, [id]: { ...d[id], ...patch } })); setSaved(false);
  };
  const voiceOpts = cloudOn
    ? cloudVoices.map((v) => ({ value: v.name, label: v.gender ? `${prettyVoice(v.name)} · ${v.gender}` : prettyVoice(v.name), search: v.name }))
    : voices.map((v) => ({ value: v, label: prettyVoice(v), search: v }));
  // Инициал имени (или «?») для заглушки-аватара; цвет из палитры спикеров (как кружки в TranscriptView).
  const initialOf = (name: string) => (name.trim()[0] ?? "?").toUpperCase();
  const avatarColor = (i: number) => SPK_PALETTE[i % SPK_PALETTE.length];
  async function apply() {
    setSaving(true);
    try {
      const payload = characters.map((c) => ({
        id: c.id, name: draft[c.id]?.name ?? c.name,
        speech_note: draft[c.id]?.speech_note ?? "", dub_voice: draft[c.id]?.voice ?? null,
      }));
      const r = await api.setCasting(pid, payload);
      onChange(r.characters);
      setSaved(true); playSfx("success");
    } catch (e) { useStore.getState().pushActivity(String(e), "error"); playSfx("error"); }
    finally { setSaving(false); }
  }
  // Прослушка образца голоса персонажа: одна играет за раз (стоп предыдущей). null -> ничего не играет.
  const [playingId, setPlayingId] = useState<string | null>(null);
  const sampleAudio = useRef<HTMLAudioElement | null>(null);
  useEffect(() => () => { sampleAudio.current?.pause(); }, []);   // размонтаж -> стоп звука
  const toggleSample = (id: string) => {
    const cur = sampleAudio.current;
    if (playingId === id) { cur?.pause(); setPlayingId(null); return; }   // повторный клик = стоп
    cur?.pause();
    const a = new Audio(api.castingVoiceUrl(pid, id));
    sampleAudio.current = a;
    a.onended = () => setPlayingId(null);
    a.play().then(() => setPlayingId(id)).catch(() => setPlayingId(null));
  };
  // Сохранить кастинг в библиотеку (#115): именованный профиль -> применяется к другим роликам через casting_ref.
  // Имя вводится инлайн-инпутом (DS-паттерн, как переименование записанного голоса) — без window.prompt.
  const [naming, setNaming] = useState(false);
  const [libName, setLibName] = useState("");
  const [libBusy, setLibBusy] = useState(false);
  const [libSaved, setLibSaved] = useState(false);
  async function saveToLibrary() {
    const name = libName.trim();
    if (!name) { setNaming(false); return; }
    setLibBusy(true);
    try {
      await api.saveCastingToLibrary(pid, name);
      setNaming(false); setLibName(""); setLibSaved(true); playSfx("success");
      useStore.getState().pushActivity(t("castingLib.savedToast", { name }), "work");
      window.setTimeout(() => setLibSaved(false), 2000);
    } catch (e) { useStore.getState().pushActivity(String(e), "error"); playSfx("error"); }
    finally { setLibBusy(false); }
  }
  return (
    <div className="w-full h-full flex flex-col rounded-xl bg-[var(--color-surface)] border border-[var(--color-border)] overflow-hidden">
      <div className="shrink-0 flex items-center gap-2 px-4 py-2.5 border-b border-[var(--color-border)]">
        <Users size={16} className="text-[var(--color-accent)]" />
        <span className="text-[13px] font-semibold">{t("casting.title")}</span>
        <span className="text-[11px] text-[var(--color-muted)]">{t("casting.count", { count: characters.length })}</span>
        <div className="flex-1" />
        {/* СОХРАНИТЬ КАСТИНГ В БИБЛИОТЕКУ (#115): инлайн-ввод имени (DS-паттерн), затем POST. */}
        {naming ? (
          <div className="flex items-center gap-1.5">
            <input value={libName} onChange={(e) => setLibName(e.target.value)} autoFocus
              onKeyDown={(e) => { if (e.key === "Enter") saveToLibrary(); else if (e.key === "Escape") { setNaming(false); setLibName(""); } }}
              placeholder={t("castingLib.namePlaceholder")}
              className="w-44 bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-lg px-2.5 py-1.5 text-[12px] focus:border-[var(--color-accent)] focus:outline-none" />
            <button onClick={saveToLibrary} disabled={libBusy || !libName.trim()} title={t("castingLib.save")}
              className="shrink-0 px-2.5 py-1.5 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-[12px] font-semibold disabled:opacity-50">
              {libBusy ? <Loader2 size={14} className="animate-spin" /> : <Check size={14} />}
            </button>
            <button onClick={() => { setNaming(false); setLibName(""); }} title={t("common.cancel")}
              className="shrink-0 p-1.5 rounded-lg border border-[var(--color-border)] text-[var(--color-muted)] hover:text-[var(--color-text)]">
              <X size={14} />
            </button>
          </div>
        ) : (
          <button onClick={() => setNaming(true)} title={t("castingLib.saveHint")}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-2)] text-[var(--color-muted)] text-[12px] font-medium hover:text-[var(--color-text)] hover:border-[#3a414c] transition">
            {libSaved ? <Check size={14} className="text-[var(--color-accent-2)]" /> : <Save size={14} />}
            {libSaved ? t("castingLib.savedShort") : t("castingLib.save")}
          </button>
        )}
        <button onClick={apply} disabled={saving}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-[12px] font-semibold disabled:opacity-60 hover:brightness-105 transition">
          {saving ? <Loader2 size={14} className="animate-spin" /> : saved ? <Check size={14} /> : <Users size={14} />}
          {saved ? t("casting.applied") : t("casting.apply")}
        </button>
      </div>
      <div data-kb-scroll className="flex-1 min-h-0 overflow-y-auto p-4">
        <p className="text-[12px] text-[var(--color-muted)] leading-snug mb-3 max-w-2xl">{t("casting.hint")}</p>
        <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))" }}>
          {characters.map((c, i) => {
            const d = draft[c.id] ?? { name: c.name, speech_note: c.speech_note ?? "", voice: c.voice };
            // Есть кадр (URL не null) и он не сломался -> картинка; иначе заглушка-инициал.
            const showImg = !!c.sample_frame_url && !imgFail[c.id];
            return (
              <div key={c.id} data-mcp-context={`character ${c.id}: ${d.name}`} className="rounded-xl border border-[var(--color-border)] bg-[var(--color-surface-2)] overflow-hidden flex flex-col">
                {/* Аватар: кадр из видео (2:3 постер как на Кинопоиске). Нет кадра/битая ссылка -> инициал. */}
                <div className="relative aspect-[2/3] bg-[var(--color-surface)] overflow-hidden">
                  {showImg ? (
                    <img src={api.castingAvatarUrl(pid, c.id)} alt={d.name}
                      className="w-full h-full object-cover" loading="lazy"
                      onError={() => setImgFail((m) => ({ ...m, [c.id]: true }))} />
                  ) : (
                    <div className="w-full h-full grid place-items-center"
                      style={{ background: `color-mix(in oklab, ${avatarColor(i)} 22%, var(--color-surface))` }}>
                      <span className="text-[40px] font-bold leading-none" style={{ color: avatarColor(i) }}>{initialOf(d.name)}</span>
                    </div>
                  )}
                  {/* Пол — бейдж поверх аватара. */}
                  {c.gender && (
                    <span className="absolute top-1.5 right-1.5 px-1.5 py-0.5 rounded-md text-[10px] font-semibold bg-[color-mix(in_oklab,var(--color-surface)_75%,transparent)] backdrop-blur text-[var(--color-text)]">
                      {t(GENDER_LABEL_KEY[genderKey(c.gender)])}
                    </span>
                  )}
                  {/* Образец голоса (#115): играть/стоп wav. Кнопка только если бэк отдал voice_sample_url. */}
                  {c.voice_sample_url && (
                    <button type="button" onClick={() => toggleSample(c.id)}
                      title={playingId === c.id ? t("castingLib.stopSample") : t("castingLib.playSample")}
                      className="absolute bottom-1.5 left-1.5 inline-flex items-center justify-center w-7 h-7 rounded-full bg-[color-mix(in_oklab,var(--color-surface)_75%,transparent)] backdrop-blur text-[var(--color-text)] hover:text-[var(--color-accent)] transition">
                      {playingId === c.id ? <Square size={12} /> : <Play size={12} />}
                    </button>
                  )}
                </div>
                <div className="p-2.5 flex flex-col gap-2">
                  {/* Имя персонажа. */}
                  <input value={d.name} onChange={(e) => setField(c.id, { name: e.target.value })}
                    placeholder={t("casting.namePlaceholder")}
                    className="w-full bg-[var(--color-surface)] border border-[var(--color-border)] rounded-lg px-2 py-1.5 text-[13px] font-semibold focus:border-[var(--color-accent)] focus:outline-none" />
                  {/* Кол-во реплик. */}
                  <div className="flex items-center gap-1.5 text-[11px] text-[var(--color-muted)]">
                    <FileText size={12} />{t("casting.lines", { count: c.line_count })}
                  </div>
                  {/* Характер речи — уходит в Гемму. */}
                  <div>
                    <div className="text-[10px] uppercase tracking-[0.1em] text-[var(--color-muted)] mb-1">{t("casting.speech")}</div>
                    <textarea value={d.speech_note} onChange={(e) => setField(c.id, { speech_note: e.target.value })}
                      rows={2} placeholder={t("casting.speechPlaceholder")}
                      className="w-full bg-[var(--color-surface)] border border-[var(--color-border)] rounded-lg px-2 py-1.5 text-[12px] focus:border-[var(--color-accent)] focus:outline-none resize-none" />
                  </div>
                  {/* Голос дубляжа — из библиотеки. */}
                  <div>
                    <div className="text-[10px] uppercase tracking-[0.1em] text-[var(--color-muted)] mb-1">{t("casting.voice")}</div>
                    {/* «clone»/пусто = клонировать из видео -> показываем ПУСТО (плейсхолдер), а не буквальный
                        текст «clone» (иначе значение не из списка и автокомплит выглядит сломанным). */}
                    <Combobox value={d.voice && d.voice.toLowerCase() !== "clone" ? d.voice : ""}
                      onChange={(v) => setField(c.id, { voice: v || null })} options={voiceOpts}
                      placeholder={cloudOn ? t("casting.autoByGender") : t("casting.voiceClone")} noResults={t("voice.noMatch")} allowClear size="sm" className="w-full" />
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

// Профессиональный масштабируемый и скроллируемый таймлайн (в стиле ElevenLabs / NLE)
function InteractiveTimeline({
  pid,
  duration,
  scrub,
  segments,
  playing,
  onSeek,
  onPlaySeg,
  setProject,
}: {
  pid: string;
  duration: number;
  scrub: number;
  segments: Project["segments"];
  playing: boolean;
  onSeek: (t: number) => void;
  onPlaySeg: (seg: Project["segments"][number]) => void;
  setProject: (p: Project) => void;
}) {
  const { t } = useTranslation();
  const containerRef = useRef<HTMLDivElement>(null);
  const trackRef = useRef<HTMLDivElement>(null);
  const [zoom, setZoom] = useState(60); // pixels per second
  const [draggingSeg, setDraggingSeg] = useState<{
    id: string;
    type: "move" | "resize-left" | "resize-right";
    startX: number;
    origStart: number;
    origEnd: number;
  } | null>(null);

  const total = Math.max(0.1, duration);
  const totalPx = Math.max(800, total * zoom);

  // Auto-scroll follow playhead during playback
  useEffect(() => {
    if (!containerRef.current || !playing) return;
    const playheadPx = scrub * zoom;
    const halfWidth = containerRef.current.clientWidth / 2;
    containerRef.current.scrollLeft = Math.max(0, playheadPx - halfWidth);
  }, [scrub, playing, zoom]);

  const handleWheel = (e: React.WheelEvent) => {
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      setZoom((z) => Math.max(20, Math.min(300, z + (e.deltaY < 0 ? 10 : -10))));
    } else if (containerRef.current && e.deltaX === 0) {
      containerRef.current.scrollLeft += e.deltaY;
    }
  };

  const getT = (clientX: number) => {
    if (!containerRef.current) return 0;
    const rect = containerRef.current.getBoundingClientRect();
    const scrollLeft = containerRef.current.scrollLeft;
    const x = clientX - rect.left + scrollLeft;
    return Math.max(0, Math.min(total, x / zoom));
  };

  const handlePointerDown = (
    e: React.PointerEvent,
    seg: Project["segments"][number],
    type: "move" | "resize-left" | "resize-right"
  ) => {
    e.stopPropagation();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    setDraggingSeg({
      id: seg.id,
      type,
      startX: e.clientX,
      origStart: seg.start,
      origEnd: seg.end,
    });
  };

  const [peaks, setPeaks] = useState<number[]>([]);
  useEffect(() => {
    api.waveform(pid).then((r) => setPeaks(r.peaks || [])).catch(() => {});
  }, [pid]);

  const handlePointerMove = (e: React.PointerEvent) => {
    if (!draggingSeg) return;
    const dt = (e.clientX - draggingSeg.startX) / zoom;
    let newStart = draggingSeg.origStart;
    let newEnd = draggingSeg.origEnd;

    if (draggingSeg.type === "move") {
      const len = draggingSeg.origEnd - draggingSeg.origStart;
      newStart = Math.max(0, draggingSeg.origStart + dt);
      newEnd = newStart + len;
    } else if (draggingSeg.type === "resize-left") {
      newStart = Math.max(0, Math.min(draggingSeg.origEnd - 0.1, draggingSeg.origStart + dt));
    } else if (draggingSeg.type === "resize-right") {
      newEnd = Math.max(newStart + 0.1, draggingSeg.origEnd + dt);
    }

    const cur = useStore.getState().project;
    if (!cur) return;

    const SNAP_THRESH = 0.15;

    // 1. Прилипание к вспышкам волновой дорожки (Waveform Snapping)
    if (peaks.length > 0 && total > 0) {
      const step = total / peaks.length;
      const sIdx = Math.max(0, Math.floor((newStart - 0.2) / step));
      const eIdx = Math.min(peaks.length - 1, Math.ceil((newStart + 0.2) / step));
      for (let i = sIdx; i <= eIdx; i++) {
        if (peaks[i] > 0.08) {
          const peakT = i * step;
          if (draggingSeg.type === "move" || draggingSeg.type === "resize-left") {
            if (Math.abs(newStart - peakT) < SNAP_THRESH) {
              newStart = Math.round(peakT * 100) / 100;
              if (draggingSeg.type === "move") newEnd = newStart + (draggingSeg.origEnd - draggingSeg.origStart);
              break;
            }
          }
        }
      }
      const esIdx = Math.max(0, Math.floor((newEnd - 0.2) / step));
      const eeIdx = Math.min(peaks.length - 1, Math.ceil((newEnd + 0.2) / step));
      for (let i = esIdx; i <= eeIdx; i++) {
        if (peaks[i] > 0.08) {
          const peakT = i * step;
          if (draggingSeg.type === "move" || draggingSeg.type === "resize-right") {
            if (Math.abs(newEnd - peakT) < SNAP_THRESH) {
              newEnd = Math.round(peakT * 100) / 100;
              break;
            }
          }
        }
      }
    }

    // 2. Магнитное авто-прилипание к границам соседних субтитров
    for (const s of cur.segments) {
      if (s.id === draggingSeg.id) continue;
      if (draggingSeg.type === "move" || draggingSeg.type === "resize-left") {
        if (Math.abs(newStart - s.end) < SNAP_THRESH) newStart = s.end;
        else if (Math.abs(newStart - s.start) < SNAP_THRESH) newStart = s.start;
        if (draggingSeg.type === "move") newEnd = newStart + (draggingSeg.origEnd - draggingSeg.origStart);
      }
      if (draggingSeg.type === "move" || draggingSeg.type === "resize-right") {
        if (Math.abs(newEnd - s.start) < SNAP_THRESH) newEnd = s.start;
        else if (Math.abs(newEnd - s.end) < SNAP_THRESH) newEnd = s.end;
      }
    }

    const updatedSegs = cur.segments.map((s) =>
      s.id === draggingSeg.id ? { ...s, start: newStart, end: newEnd, dirty: true } : s
    );
    useStore.getState().setProject({ ...cur, segments: updatedSegs });
  };

  const handlePointerUp = async (e: React.PointerEvent) => {
    if (!draggingSeg) return;
    try { (e.target as HTMLElement).releasePointerCapture(e.pointerId); } catch { /* ignore */ }
    const cur = useStore.getState().project;
    const targetSeg = cur?.segments.find((s) => s.id === draggingSeg.id);
    setDraggingSeg(null);
    if (targetSeg) {
      try {
        const fresh = await api.patch(pid, {
          op: "segment",
          id: targetSeg.id,
          start: targetSeg.start,
          end: targetSeg.end,
        });
        setProject(fresh);
      } catch { /* ignore */ }
    }
  };

  return (
    <div className="flex flex-col gap-1 w-full">
      {/* Zoom Toolbar */}
      <div className="flex items-center justify-between px-1 text-[11px] text-[var(--color-muted)]">
        <span className="font-semibold flex items-center gap-1.5">
          <AudioLines size={13} className="text-[var(--color-accent)]" />
          {t("timeline.title")}
        </span>
        <div className="flex items-center gap-2">
          <button
            onClick={() => setZoom((z) => Math.max(20, z - 15))}
            title={t("timeline.zoomOut")}
            className="p-1 rounded bg-[var(--color-surface-2)] border border-[var(--color-border)] hover:border-[var(--color-accent)]"
          >
            <ZoomOut size={13} />
          </button>
          <span className="mono text-[10px] w-12 text-center">{zoom} px/s</span>
          <button
            onClick={() => setZoom((z) => Math.min(300, z + 15))}
            title={t("timeline.zoomIn")}
            className="p-1 rounded bg-[var(--color-surface-2)] border border-[var(--color-border)] hover:border-[var(--color-accent)]"
          >
            <ZoomIn size={13} />
          </button>
        </div>
      </div>

      {/* Scrollable Track Canvas Container */}
      <div
        ref={containerRef}
        onWheel={handleWheel}
        onClick={(e) => onSeek(getT(e.clientX))}
        className="relative w-[#full] h-24 bg-[var(--color-surface-2)]/60 border border-[var(--color-border)] rounded-xl overflow-x-auto overflow-y-hidden select-none cursor-pointer"
      >
        <div ref={trackRef} className="relative h-full" style={{ width: `${totalPx}px` }}>
          {/* Waveform Background */}
          <WaveformTimeline pid={pid} duration={duration} scrub={scrub} segments={segments} gainDb={0} onSeek={onSeek} />

          {/* Segment Audio & Subtitle Blocks */}
          {segments.map((seg) => {
            const leftPx = seg.start * zoom;
            const widthPx = Math.max(36, (seg.end - seg.start) * zoom);
            const isActive = scrub >= seg.start && scrub <= seg.end;
            return (
              <div
                key={seg.id}
                data-timeline-seg={seg.id}
                style={{ left: `${leftPx}px`, width: `${widthPx}px` }}
                onPointerDown={(e) => handlePointerDown(e, seg, "move")}
                onPointerMove={handlePointerMove}
                onPointerUp={handlePointerUp}
                className={`absolute top-1.5 bottom-1.5 rounded-lg border flex items-center justify-between px-1 text-[11px] cursor-grab active:cursor-grabbing transition-colors shadow-md ${
                  isActive
                    ? "bg-[var(--color-accent)]/30 border-[var(--color-accent)] text-[var(--color-text)] ring-2 ring-[var(--color-accent)]/80 backdrop-blur"
                    : "bg-[var(--color-surface-2)]/95 border-[var(--color-border)] text-[var(--color-text)] hover:border-[var(--color-accent)]/70"
                }`}
              >
                {/* Left Trim Handle */}
                <div
                  onPointerDown={(e) => handlePointerDown(e, seg, "resize-left")}
                  className="w-2 h-full bg-white/20 hover:bg-[var(--color-accent)] cursor-col-resize shrink-0 rounded-l-md flex items-center justify-center text-[8px] opacity-60 hover:opacity-100"
                  title={t("timeline.editStart")}
                >
                  │
                </div>

                {/* Content: Play Button + Text + Speaker Badge */}
                <div className="flex items-center gap-1.5 min-w-0 flex-1 px-1 overflow-hidden">
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      onPlaySeg(seg);
                    }}
                    title={t("timeline.listen")}
                    className="p-1 rounded-md bg-[var(--color-accent)] text-[var(--color-on-accent)] shrink-0 hover:scale-105 transition-transform"
                  >
                    <Play size={11} />
                  </button>
                  {seg.speaker != null && (
                    <span className="mono text-[9px] px-1 py-0.5 rounded bg-[var(--color-overlay)] text-[var(--color-muted)] shrink-0">
                      SPK {seg.speaker}
                    </span>
                  )}
                  <span className="font-medium truncate text-[11px] flex-1 leading-snug">
                    {seg.tgt_text || seg.src_text}
                  </span>
                </div>

                {/* Right Trim Handle */}
                <div
                  onPointerDown={(e) => handlePointerDown(e, seg, "resize-right")}
                  className="w-2 h-full bg-white/20 hover:bg-[var(--color-accent)] cursor-col-resize shrink-0 rounded-r-md flex items-center justify-center text-[8px] opacity-60 hover:opacity-100"
                  title={t("timeline.editEnd")}
                >
                  │
                </div>
              </div>
            );
          })}

          {/* Red Playhead Line */}
          <div
            className="absolute top-0 bottom-0 w-0.5 bg-[var(--color-accent)] z-20 pointer-events-none shadow-[0_0_8px_var(--color-accent)]"
            style={{ left: `${scrub * zoom}px` }}
          />
        </div>
      </div>
    </div>
  );
}

function parseSrtTime(tStr: string): number {
  const clean = tStr.trim().replace(",", ".");
  const parts = clean.split(":");
  if (parts.length === 3) {
    const h = parseFloat(parts[0]) || 0;
    const m = parseFloat(parts[1]) || 0;
    const s = parseFloat(parts[2]) || 0;
    return h * 3600 + m * 60 + s;
  } else if (parts.length === 2) {
    const m = parseFloat(parts[0]) || 0;
    const s = parseFloat(parts[1]) || 0;
    return m * 60 + s;
  }
  return parseFloat(clean) || 0;
}

function parseSrtAssText(content: string): { start: number; end: number; speaker: string; text: string }[] {
  const results: { start: number; end: number; speaker: string; text: string }[] = [];
  const lines = content.replace(/\r\n/g, "\n").split("\n");

  const isAss = lines.some((l) => l.trim().startsWith("[Events]") || l.trim().startsWith("Dialogue:"));

  if (isAss) {
    let startIdx = 1, endIdx = 2, nameIdx = 4, textIdx = 9;
    for (const line of lines) {
      const trimmed = line.trim();
      if (trimmed.startsWith("Format:")) {
        const fields = trimmed.substring(7).split(",").map((f) => f.trim().toLowerCase());
        const s = fields.indexOf("start"); if (s !== -1) startIdx = s;
        const e = fields.indexOf("end"); if (e !== -1) endIdx = e;
        const n = fields.indexOf("name"); if (n !== -1) nameIdx = n;
        const t = fields.indexOf("text"); if (t !== -1) textIdx = t;
      } else if (trimmed.startsWith("Dialogue:")) {
        const parts = trimmed.substring(9).split(",");
        if (parts.length > textIdx) {
          const startStr = parts[startIdx]?.trim() || "0";
          const endStr = parts[endIdx]?.trim() || "0";
          const speaker = parts[nameIdx]?.trim() || "0";
          const textRaw = parts.slice(textIdx).join(",");
          const textClean = textRaw.replace(/\\N/gi, " ").replace(/\{[^}]+\}/g, "").trim();
          const start = parseSrtTime(startStr);
          const end = parseSrtTime(endStr);
          if (end > start) {
            results.push({ start, end, speaker: speaker || "0", text: textClean });
          }
        }
      }
    }
  } else {
    const blocks = content.replace(/\r\n/g, "\n").split(/\n\s*\n/);
    for (const block of blocks) {
      const bLines = block.split("\n").map((l) => l.trim()).filter(Boolean);
      if (bLines.length < 2) continue;
      const timeLineIdx = bLines.findIndex((l) => l.includes("-->"));
      if (timeLineIdx === -1) continue;

      const timeLine = bLines[timeLineIdx];
      const timeParts = timeLine.split("-->").map((s) => s.trim());
      if (timeParts.length < 2) continue;

      const start = parseSrtTime(timeParts[0]);
      const end = parseSrtTime(timeParts[1]);

      const rawText = bLines.slice(timeLineIdx + 1).join(" ");
      const textClean = rawText.replace(/<[^>]+>/g, "").trim();

      if (end > start && textClean) {
        results.push({ start, end, speaker: "0", text: textClean });
      }
    }
  }

  return results.sort((a, b) => a.start - b.start);
}

// Что пользователь печатает прямо сейчас (burst-снимок undo): не затирается, когда проект перечитывается под ним.
function draftOf(burst: string | null): Draft {
  if (burst?.startsWith("seg:")) return { segment: burst.slice(4) };
  if (burst?.startsWith("title:")) return { title: Number(burst.slice(6)) };
  return null;
}

function Editor() {
  const { t, i18n } = useTranslation();
  const p = useStore((s) => s.project) as Project;
  const rev = useStore((s) => s.rev);   // for the compare 'result' pane refetch (PreviewCanvas reads rev itself)        // subscribe ONLY to what we render -> no re-render on
  const pid = useStore((s) => s.pid) as string;           // rev bumps or progress SSE ticks (those go to PreviewCanvas)
  const rendered = useStore((s) => s.rendered);
  const setProject = useStore((s) => s.setProject);
  const setRendered = useStore((s) => s.setRendered);
  const setStage = useStore((s) => s.setStage);
  const bump = useStore((s) => s.bump);
  // Экспорт на другие языки: поповер с мультиселектом -> клон+ре-перевод+рендер (наследует правки).
  const [langMenu, setLangMenu] = useState(false);
  const [exportLangs, setExportLangs] = useState<string[]>([]);
  const [, forcePortal] = useState(0);
  useEffect(() => { forcePortal((n) => n + 1); }, []);   // 2-й рендер: слоты TopBar (#editor-modes-slot/#editor-actions-slot) уже в DOM
  const selBlur = useStore((s) => s.selBlur);             // selected blur zone (shared with the canvas overlay)
  const setSelBlur = useStore((s) => s.setSelBlur);
  const selTitle = useStore((s) => s.selTitle);           // selected title (shared with the canvas overlay)
  const setSelTitle = useStore((s) => s.setSelTitle);
  const pushActivity = useStore((s) => s.pushActivity);   // журнал действий (статус-строка в шапке)
  const rendering = useStore((s) => s.rendering);
  const setRendering = useStore((s) => s.setRendering);
  const addExport = useStore((s) => s.addExport);
  const updateExport = useStore((s) => s.updateExport);
  const pushHistory = useStore((s) => s.pushHistory);
  const undo = useStore((s) => s.undo);
  const redo = useStore((s) => s.redo);
  const canUndo = useStore((s) => s.past.length > 0);
  const canRedo = useStore((s) => s.future.length > 0);
  const [scrub, setScrub] = useState(initialScrub);                   // ?t=SEC — deep-link на кадр (для скринов/шаринга), иначе 1.0с
  const [selSegs, setSelSegs] = useState<Set<string>>(new Set());     // multi-select for bulk hide/delete
  const [selBlurs, setSelBlurs] = useState<Set<number>>(new Set());   // multi-select: mask boxes
  const [selTitles, setSelTitles] = useState<Set<number>>(new Set()); // multi-select: titles
  const [fonts, setFonts] = useState<Record<string, string>>({});
  const [voiceList, setVoiceList] = useState<string[]>([]);
  const [spkVoiceBusy, setSpkVoiceBusy] = useState<string | null>(null);
  const [voicePreview, setVoicePreview] = useState<string | null>(null);   // имя проигрываемого сэмпла голоса
  const voicePreviewAudio = useRef<HTMLAudioElement | null>(null);
  // Прослушать/остановить сэмпл выбранного голоса (плей-пауза у дропдауна).
  const toggleVoicePreview = (name: string) => {
    if (!name) return;
    const cur = voicePreviewAudio.current;
    if (voicePreview === name) { cur?.pause(); setVoicePreview(null); return; }   // повторный клик = пауза
    cur?.pause();
    const a = new Audio(api.voiceSampleUrl(name));
    voicePreviewAudio.current = a;
    a.onended = () => setVoicePreview(null);
    a.play().then(() => setVoicePreview(name)).catch(() => setVoicePreview(null));
  };
  const [gainDraft, setGainDraft] = useState<number | null>(null);
  const [voGainDraft, setVoGainDraft] = useState<number | null>(null);   // черновик громкости оригинала (voiceover)
  const [presets, setPresets] = useState<Record<string, Record<string, unknown>>>({});
  useEffect(() => { api.fonts().then((r) => setFonts(r.fonts)).catch(() => {}); }, []);   // bundled caption fonts
  // голоса из каталога; если пусто и ещё не пробовали — тихо тянем дефолтный пак (VibeVoice, ~100МБ) в фоне
  useEffect(() => {
    api.voices().then(async (r) => {
      setVoiceList(r.voices);
      if (r.voices.length === 0 && !localStorage.getItem("voicepack-auto")) {
        localStorage.setItem("voicepack-auto", "1");
        try {
          const { job_id } = await api.voicesDownloadPack();
          await api.watchJob(job_id, () => {});
          const v = await api.voices(); setVoiceList(v.voices);
        } catch { /* тихо: пак опционален */ }
      }
    }).catch(() => {});
  }, []);
  useEffect(() => { api.presets().then((r) => setPresets(r.presets)).catch(() => {}); }, []);   // caption look presets
  const [sizeDraft, setSizeDraft] = useState<number | null>(null);   // live size while dragging (commit on release)
  const [lane, setLane] = useState<"subs" | "blur" | "titles">("subs"); // left lane: which object type to edit
  // Кастинг персонажей (#115): вкладка «Персонажи» в редакторе. Список тянем при монтировании; если бэк
  // вернул персонажей (casting включался на анализе) — показываем вкладку-переключатель в тулбаре.
  const [characters, setCharacters] = useState<Character[] | null>(null);
  const [castView, setCastView] = useState(false);   // главная область: сетка карточек вместо превью
  useEffect(() => { api.casting(pid).then((r) => setCharacters(r.characters)).catch(() => setCharacters([])); }, [pid]);
  const hasCasting = !!characters && characters.length > 0;
  const [blurAll, setBlurAll] = useState(false);                      // blur: only active-on-frame vs all zones
  const [compare, setCompare] = useState(false);                      // before/after split preview (Topaz-style)
  const [play, setPlay] = useState(false);                            // dub playback: play TTS audio + advance preview frames + playhead
  const [showHelp, setShowHelp] = useState(false);                    // оверлей-шпаргалка хоткеев (?)
  const previewRef = useRef<HTMLDivElement>(null);                    // контейнер превью -> фулскрин по F + Ctrl-колесо
  const scrubRef = useRef(0);                                         // свежий scrub без stale-замыкания в хоткеях
  const audioRef = useRef<HTMLAudioElement>(null);
  const [vol, setVol] = useState<number>(() => { const s = localStorage.getItem("dub-vol"); return s ? parseFloat(s) : 1; });

  const playEndRef = useRef<number>(Infinity);                        // stop time for single-phrase playback (Infinity = full)
  const dubRev = useStore((s) => s.dubRev);
  const bumpDub = useStore((s) => s.bumpDub);
  useEffect(() => {
    if (audioRef.current) {
      audioRef.current.volume = vol;
      audioRef.current.playbackRate = 1.0;
    }
  }, [vol, dubRev]);   // применить громкость прослушки (скорость звука ВСЕГДА 1.0x)
  // Dub playback — drive the EDITOR preview from the dub audio track (no video element): the preview frame and the
  // waveform playhead follow audio.currentTime, so you hear the dub while the future result plays frame-by-frame.
  useEffect(() => {
    const a = audioRef.current; if (!a) return;
    if (!play) { a.pause(); return; }
    setRendered(false);
    a.playbackRate = 1.0;
    a.play().catch(() => {});
    const id = window.setInterval(() => {
      if (a.currentTime >= playEndRef.current) { a.pause(); setPlay(false); }   // single phrase -> stop at its end
      else setScrub(a.currentTime);
    }, 150);
    return () => window.clearInterval(id);
  }, [play]);   // eslint-disable-line react-hooks/exhaustive-deps
  const [regenId, setRegenId] = useState<string | null>(null);        // segment whose TTS is being re-generated
  const [shortening, setShortening] = useState<string | null>(null);  // строка (или "__all__"), чей перевод сокращается под слот
  const [onlyOver, setOnlyOver] = useState(false);                    // фильтр списка: только не влезающие в слот
  const [takesOpen, setTakesOpen] = useState<string | null>(null);    // строка с раскрытой историей дублей
  const [remixText, setRemixText] = useState("");                     // funny-remix theme/instruction for Gemma
  const [remixing, setRemixing] = useState(false);
  const defaultSubStyle = {
    color: "#FFFFFF",
    outline: "#000000",
    italic: false,
    bold: false,
    uppercase: false,
    font: "Montserrat",
    align: "center",
    size_px: Math.round((p.meta.height || 1080) / 14),
    outline_w: 2,
    shadow_dir: null,
    scene_flat: false,
    plate: false,
    plate_color: "#00000080",
  };
  const ss = { ...defaultSubStyle, ...(p.captions.sub_style || {}) };

  async function handleSaveSubtitles() {
    try {
      const srtTime = (s: number) => {
        const ms = Math.max(0, Math.round(s * 1000)), z = (n: number, w = 2) => String(n).padStart(w, "0");
        return `${z(Math.floor(ms / 3600000))}:${z(Math.floor((ms % 3600000) / 60000))}:${z(Math.floor((ms % 60000) / 1000))},${z(ms % 1000, 3)}`;
      };
      const isDub = p.mode === "dub" || p.mode === "voiceover";
      const originalTop = p.subs.bilingual.order === "original_top";
      const own = new Map(p.captions.overrides.flatMap((o) => (o.text != null ? [[o.seg_id, o.text] as const] : [])));
      const content = p.segments
        .filter((s) => !s.hidden && !s.keep_original)
        .map((s) => ({ s, text: subtitleText({ src_text: s.src_text, tgt_text: own.get(s.id) ?? s.tgt_text }, p.subs.mode, isDub, originalTop) }))
        .filter(({ text }) => text)
        .map(({ s, text }, i) => `${i + 1}\n${srtTime(s.start)} --> ${srtTime(s.end)}\n${text}\n`).join("\n");
      await api.putProject(pid, p);

      if ("showSaveFilePicker" in window) {
        try {
          const handle = await (window as unknown as { showSaveFilePicker: (opts: unknown) => Promise<FileSystemFileHandle> }).showSaveFilePicker({
            suggestedName: "subtitles.srt",
            types: [{ description: t("subs.saveTypeDesc"), accept: { "text/plain": [".srt"] } }],
          });
          const writable = await handle.createWritable();
          await writable.write(content);
          await writable.close();
          pushActivity(t("subs.saved"), "done");
          playSfx("success");
          return;
        } catch (e: unknown) {
          if ((e as { name?: string }).name === "AbortError") return;
        }
      }

      await api.saveText(pid, "subtitles.srt", content);
      pushActivity(t("subs.savedAll"), "done");
      playSfx("success");
    } catch (err) {
      await surfaceErr(err);
    }
  }

  async function handleImportSubtitles(file: File) {
    try {
      const text = await file.text();
      const parsed = parseSrtAssText(text);
      if (parsed.length === 0) {
        pushActivity(t("subs.empty"), "error");
        playSfx("error");
        return;
      }

      pushHistory(p);
      setRendered(false);

      const newSegments: Project["segments"] = parsed.map((item, idx) => ({
        id: `s${idx + 1}`,
        start: Math.max(0, item.start),
        end: Math.max(item.start + 0.1, item.end),
        speaker: item.speaker || "0",
        src_text: "",
        tgt_text: item.text,
        voice: null,
        dirty: true,
      }));

      const updatedProj: Project = { ...p, segments: newSegments };
      const saved = await api.putProject(pid, updatedProj);
      setProject(saved);
      bump();
      pushActivity(t("subs.loaded", { count: parsed.length, file: file.name }), "done");
      playSfx("success");
    } catch (err) {
      await surfaceErr(err);
    }
  }


  // уникальные спикеры проекта (для переброса фразы другому спикеру на плашке; голос спикера — в настройках голосов)
  const speakers = Array.from(new Set(p.segments.map((s) => s.speaker).filter((s): s is string => s != null && s !== "")))
    .sort((a, b) => a.localeCompare(b, undefined, { numeric: true }));
  // one undo snapshot per edit BURST (focus->type->blur), segments AND titles: snapshot on the FIRST change of a
  // field, keyed by field, cleared on blur. Not on focus (that killed redo) nor per-keystroke (that flooded history).
  const burstRef = useRef<string | null>(null);

  function patchSeg(id: string, tgt: string) {                       // instant local echo while typing
    if (burstRef.current !== `seg:${id}`) { pushHistory(p); burstRef.current = `seg:${id}`; }
    setProject({ ...p, segments: p.segments.map((x) => x.id === id ? { ...x, tgt_text: tgt, dirty: true } : x) });
  }
  function titleText(i: number, text: string) {                      // instant local echo. РЕНДЕР берёт tgt (если непустой), поэтому правим И tgt И text — иначе правка текста не видна на кадре
    if (burstRef.current !== `title:${i}`) { pushHistory(p); burstRef.current = `title:${i}`; }
    setProject({ ...p, captions: { ...p.captions, titles: p.captions.titles.map((x, j) => j === i ? { ...x, text, tgt: text } : x) } });
  }
  // a patch/PUT rejected (4xx/5xx/offline) -> surface it in the Files panel (like doExport) and re-sync from
  // the server so the optimistic local echo can't silently diverge from persisted truth
  async function surfaceErr(err: unknown) {
    // PUT снимка (undo, импорт субтитров) на проект, который после него сохранили агент или другое окно: сервер
    // его не принял — показываем актуальный проект, а старые снимки undo больше не годятся.
    const changed = err instanceof ApiError && err.code === "project_changed";
    pushActivity(changed ? t("bridge.undoRefused") : String(err), changed ? "agent" : "error"); playSfx("error");
    if (changed) useStore.getState().resetHistory();
    else addExport({ id: `err-${Date.now()}`, name: t("common.error"), status: "error", msg: String(err) });
    try { setProject(await api.getProject(pid)); } catch { /* offline -> keep optimistic state */ }
  }
  // Джобу отменили кнопкой в полосе джоб: не ошибка — строка в журнале и проект в том виде, в каком его оставила джоба.
  async function jobCancelled(kind: JobKind) {
    pushActivity(t("jobs.cancelledKind", { kind: t(`jobs.kind.${kind}`) }), "done");
    try { setProject(await api.getProject(pid)); bump(); }
    catch (err) { pushActivity(String(err), "error"); }
  }
  async function persistSeg(id: string, tgt: string) {               // on blur -> persist to backend + refresh frame
    setRendered(false);
    const wasPinned = p.segments.find((x) => x.id === id)?.takes?.pinned != null;
    try {
      const fresh = await api.patch(pid, { op: "segment", id, tgt_text: tgt });
      setProject(fresh); bump();
      if (wasPinned && fresh.segments.find((x) => x.id === id)?.takes?.pinned == null) pushActivity(t("takes.unpinnedByEdit", { count: 1 }), "work");
    }
    catch (err) { await surfaceErr(err); }
  }
  async function branch(op: string, extra: Record<string, unknown> = {}) {
    pushHistory(p);                                                  // snapshot for undo BEFORE the mutation
    setRendered(false);
    try { const fresh = await api.patch(pid, { op, ...extra }); setProject(fresh); bump(); return fresh; }   // style/voice/text change -> re-fetch the frame
    catch (err) { await surfaceErr(err); return null; }
  }
  async function addSeg() {                                          // добавить СВОЮ фразу — пустой dirty-сегмент, текст вписывается ниже
    if (regenId) return;
    const segs = p.segments;
    const start = segs.length ? segs[segs.length - 1].end : 0;       // в конец таймлайна по умолчанию
    const speaker = segs[0]?.speaker ?? "0";                         // голос первого спикера (клон)
    pushHistory(p); setRendered(false);
    try { setProject(await api.patch(pid, { op: "add_segment", id: `u${Date.now().toString(36)}`, start, end: start + 2, speaker })); bump(); }
    catch (e) { await surfaceErr(e); }
  }
  // По проекту уже идёт озвучка или экспорт: новая джоба ждёт её конца (журнал говорит, чего ждём).
  const waitNote = (kind: JobKind) => pushActivity(t("jobs.waitingFor", { kind: t(`jobs.kind.${kind}`) }), "work");
  const watchDub = (jobId: string) => watchLocal(pid, "dub_audio", jobId, (e) => {
    if (e.type === "progress") {
      if (e.msg) useStore.getState().pushActivity(e.msg, "work");
      useStore.getState().setProgress(e.stage || "tts", e.msg || "", e.pct ?? null);
    }
  });
  async function doRegen(segId: string) {                            // re-synthesize the TTS for ONE phrase (mark dirty -> /render)
    if (regenId) return;
    setRegenId(segId); pushActivity(t("seg.regen"));
    try {
      await api.patch(pid, { op: "regen", id: segId });
      const { job_id } = await enqueueWhenFree(() => api.dubAudio(pid), waitNote);   // ре-TTS ТОЛЬКО dirty-сегмент -> свежая озвучка (без сборки видео; финал — на Экспорте)
      await watchDub(job_id);
      setProject(await api.getProject(pid)); setRendered(false); bump(); bumpDub(); playSfx("notify");   // refresh preview + reload the re-rendered dub audio
    } catch (e) { if (e instanceof JobCancelledError) await jobCancelled("dub_audio"); else await surfaceErr(e); }
    finally { setRegenId(null); }
  }
  // Сократить перевод под слот (джоба shorten): одной строки или всех, что не влезают. Новый текст — dirty,
  // озвучивается кнопкой/экспортом; снапшот до джобы — undo возвращает прежний текст.
  async function doShorten(target: { ids: string[] } | { all_over: true }, tag: string) {
    if (regenId || shortening) return;
    pushHistory(p); setShortening(tag); pushActivity(t("fit.shortening"));
    try {
      const { job_id } = await enqueueWhenFree(() => api.shorten(pid, target), waitNote);
      const res = await watchLocal(pid, "shorten", job_id, (e) => { if (e.type === "progress" && e.msg) useStore.getState().pushActivity(e.msg, "work"); }) as ShortenResult;
      setProject(await api.getProject(pid)); setRendered(false); bump();
      pushActivity(t("fit.shortened", { count: res.shortened.length }), "done");
      if (res.rejected.length) pushActivity(t("fit.rejected", { count: res.rejected.length }), "work");
      if (res.failed.length) pushActivity(t("fit.failed", { count: res.failed.length, error: res.failed[0].error }), "error");
      if (res.unpinned.length) pushActivity(t("takes.unpinnedByEdit", { count: res.unpinned.length }), "work");
      playSfx("notify");
    } catch (e) { if (e instanceof JobCancelledError) await jobCancelled("shorten"); else await surfaceErr(e); }
    finally { setShortening(null); }
  }
  // Выбрать дубль из истории фразы: без переозвучки, только пересборка микса. Дубль другого текста возвращает текст (undo — снапшот).
  // Отказ выбора получает вызвавший; пересборка микса идёт следом.
  async function selectTake(segId: string, n: number): Promise<Project> {
    pushHistory(p); setRegenId(segId); setRendered(false); pushActivity(t("takes.selecting"));
    let fresh: Project;
    try { fresh = await api.patch(pid, { op: "take_select", id: segId, take: n }); }
    catch (e) { setRegenId(null); throw e; }
    setProject(fresh); bump();
    void (async () => {
      try {
        const { job_id } = await enqueueWhenFree(() => api.dubAudio(pid), waitNote);
        await watchDub(job_id);
        setProject(await api.getProject(pid)); bumpDub(); playSfx("notify");
      } catch (e) { if (e instanceof JobCancelledError) await jobCancelled("dub_audio"); else await surfaceErr(e); }
      finally { setRegenId(null); }
    })();
    return fresh;
  }
  async function doTakeSelect(segId: string, n: number) {
    if (regenId) return;
    try { await selectTake(segId, n); } catch (e) { await surfaceErr(e); }
  }
  async function pinTake(segId: string, pinned: boolean): Promise<Project> {
    const fresh = await api.patch(pid, { op: "take_pin", id: segId, pinned });
    setProject(fresh);
    return fresh;
  }
  async function doTakePin(segId: string, pinned: boolean) {
    try { await pinTake(segId, pinned); } catch (e) { await surfaceErr(e); }
  }
  // hide/del/keep одной строки: патч проекта (без авто-ре-озвучки; рендер — по кнопке)
  async function segOp(segId: string, op: string) {
    if (regenId) return;
    pushHistory(p); setRegenId(segId); setRendered(false);
    try {
      const fresh = await api.patch(pid, { op, id: segId });
      setProject(fresh); bump();
    } catch (e) { await surfaceErr(e); }
    finally { setRegenId(null); }
  }
  async function doHideSeg(segId: string) { return segOp(segId, "hide_segment"); }   // toggle a line off/on
  async function doDelSeg(segId: string) { return segOp(segId, "del_segment"); }     // delete a line entirely (undoable)
  async function doKeepSeg(segId: string) { return segOp(segId, "keep_segment"); }   // toggle 'keep original audio'
  // Как branch(), но ошибку не показывает, а отдаёт вызвавшему: агент получает её ответом инструмента.
  async function applyEdit(op: string, fields: Record<string, unknown>): Promise<Project> {
    const cur = useStore.getState().project;
    if (cur) pushHistory(cur);
    setRendered(false);
    const fresh = await api.patch(pid, { op, ...fields });
    setProject(fresh); bump();
    return fresh;
  }
  async function doSplit(seg: Project["segments"][number]) {
    if (regenId) return;
    try { await applyEdit("split_segment", { id: seg.id, at: scrub }); } catch (err) { await surfaceErr(err); }
  }
  async function doMergeNext(seg: Project["segments"][number]) {
    const next = p.segments[p.segments.findIndex((x) => x.id === seg.id) + 1];
    if (regenId || !next) return;
    try { await applyEdit("merge_segments", { ids: [seg.id, next.id] }); } catch (err) { await surfaceErr(err); }
  }
  async function bulkDelIdx(op: "del_titles" | "del_blurs", idxs: Set<number>, clear: () => void) {
    if (!idxs.size) return;                                           // bulk-delete several titles / mask boxes by index
    pushHistory(p); setRendered(false);
    try {
      setProject(await api.patch(pid, { op, idxs: [...idxs] })); bump(); clear();
      if (op === "del_blurs") setSelBlur(null); else setSelTitle(null);   // индексы съехали -> сбросить одиночный выбор
    }
    catch (e) { await surfaceErr(e); }
  }
  async function bulkSeg(op: "del_segments" | "hide_segments" | "keep_segments", extra: Record<string, unknown> = {}) {
    if (!selSegs.size || regenId) return;                            // bulk hide/delete the selected lines
    pushHistory(p); setRegenId("__bulk__"); setRendered(false);
    try {
      const fresh = await api.patch(pid, { op, ids: [...selSegs], ...extra });
      setProject(fresh); bump(); setSelSegs(new Set());
    } catch (e) { await surfaceErr(e); }
    finally { setRegenId(null); }
  }
  // гейн дорожки/оригинала: патч без авто-пересведения (применяется при перегенерации/экспорте)
  async function applyGain(op: string, gainDb: number, label: string) {
    if (regenId) return;
    setRegenId("__all__"); pushActivity(label);
    try {
      const fresh = await api.patch(pid, { op, gain_db: gainDb });
      setProject(fresh); setRendered(false);
    } catch (e) { await surfaceErr(e); }
    finally { setRegenId(null); }
  }
  const [dragSegId, setDragSegId] = useState<string | null>(null);
  async function moveSeg(segId: string, dir: "up" | "down") {
    const idx = p.segments.findIndex((s) => s.id === segId);
    if (idx === -1) return;
    const targetIdx = dir === "up" ? idx - 1 : idx + 1;
    if (targetIdx < 0 || targetIdx >= p.segments.length) return;
    const newSegs = [...p.segments];
    const [moved] = newSegs.splice(idx, 1);
    newSegs.splice(targetIdx, 0, moved);
    const newIds = newSegs.map((s) => s.id);
    setProject({ ...p, segments: newSegs });
    try { setProject(await api.patch(pid, { op: "reorder_segments", ids: newIds })); }
    catch (e) { await surfaceErr(e); }
  }
  async function dropSeg(targetId: string) {
    if (!dragSegId || dragSegId === targetId) return;
    const fromIdx = p.segments.findIndex((s) => s.id === dragSegId);
    const toIdx = p.segments.findIndex((s) => s.id === targetId);
    if (fromIdx === -1 || toIdx === -1) return;
    const newSegs = [...p.segments];
    const [moved] = newSegs.splice(fromIdx, 1);
    newSegs.splice(toIdx, 0, moved);
    const newIds = newSegs.map((s) => s.id);
    setDragSegId(null);
    setProject({ ...p, segments: newSegs });
    try { setProject(await api.patch(pid, { op: "reorder_segments", ids: newIds })); }
    catch (e) { await surfaceErr(e); }
  }
  async function doGain(gainDb: number) { return applyGain("gain", gainDb, t("voice.gain")); }
  async function setLoudness(on: boolean) {
    if (regenId) return;
    setRegenId("__all__"); pushActivity(t("voice.loudness"));
    try {
      const fresh = await api.patch(pid, { op: "loudness", on });
      setProject(fresh); setRendered(false);
    } catch (e) { await surfaceErr(e); }
    finally { setRegenId(null); }
  }                 // монтажный гейн всей дорожки
  async function doVoiceoverGain(gainDb: number) { return applyGain("voiceover_gain", gainDb, t("voice.origGain")); }  // громкость оригинала под переводом
  async function doRegenAll() {                                      // re-synthesize the WHOLE dub (after switching the pack voice/speaker, or to re-roll)
    if (regenId) return;
    setRegenId("__all__"); pushActivity(t("voice.regenAll"));       // sentinel: disables per-seg regen buttons, no per-seg spinner
    try {
      await api.patch(pid, { op: "regen_all" });                    // mark every segment dirty
      const { job_id } = await enqueueWhenFree(() => api.dubAudio(pid), waitNote);   // ре-TTS всех сегментов -> свежая озвучка (видео на Экспорте)
      await watchDub(job_id);
      setProject(await api.getProject(pid)); setRendered(false); bump(); bumpDub(); playSfx("notify");   // покадровое превью; /dub обновлён -> плей играет новый дуб
    } catch (e) { if (e instanceof JobCancelledError) await jobCancelled("dub_audio"); else await surfaceErr(e); }
    finally { setRegenId(null); }
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
  // единый seek: скраб вейформы/слайдера + позиция dub-аудио (используется хоткеями, слайдером и вейформой)
  function onSeek(tt: number) { setRendered(false); setScrub(tt); if (audioRef.current) audioRef.current.currentTime = tt; }
  useEffect(() => { scrubRef.current = scrub; }, [scrub]);   // свежий scrub для хоткеев (без stale-замыкания)
  const setVolK = (v: number) => { setVol(v); if (audioRef.current) audioRef.current.volume = v; localStorage.setItem("dub-vol", String(v)); };
  useMediaHotkeys({
    enabled: true, duration: p.meta.duration || 0, scrubRef, seek: onSeek,
    togglePlay: playFull, previewRef, setHelp: setShowHelp, vol, setVol: setVolK,
    blocked: () => document.querySelector(".glass-scrim") != null,   // открыта палитра/модалка -> не перехватывать
  });
  async function doUndo() { const prev = undo(); if (prev) { setSelBlur(null); setSelTitle(null); setRendered(false); try { await api.putProject(pid, prev.project, prev.rev); } catch (err) { await surfaceErr(err); } bump(); } }
  async function doRedo() { const next = redo(); if (next) { setSelBlur(null); setSelTitle(null); setRendered(false); try { await api.putProject(pid, next.project, next.rev); } catch (err) { await surfaceErr(err); } bump(); } }
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
    const exId = `export-${pid}`;   // одна запись на проект (повторный экспорт заменяет её, а не плодит дубли)
    const name = baseName(p.meta.video || pid);
    addExport({ id: exId, name, status: "rendering", msg: t("common.rendering"), pid });   // queue entry -> Files panel (no screen block)
    setRendering(true); pushActivity(`${t("export.proceed")}: ${name}`);
    try {
      const { job_id } = await enqueueWhenFree(() => api.render(pid), (kind) => { updateExport(exId, { msg: t("jobs.waitingFor", { kind: t(`jobs.kind.${kind}`) }) }); waitNote(kind); });
      await watchLocal(pid, "render", job_id, (e) => { if (e.type === "progress") { updateExport(exId, { msg: e.msg || "" }); pushActivity(e.msg || "", "work"); } });
      updateExport(exId, { status: "done", msg: "", url: `${api.outputUrl(pid)}?rev=${Date.now()}` });   // bust cache on re-export
      // рендер мог сократить не влезшие фразы и записал отчёт укладки: окно берёт проект с сервера
      api.getProject(pid).then(setProject, (e: unknown) => pushActivity(String(e), "error"));
      // Раскрыть реальный выход в проводнике: контейнер может быть output.mkv (#113, сохранена ориг. дорожка) —
      // не хардкодим .mp4. Расширение из project.json (keep_original_track + container), иначе .mp4.
      const outName = p.audio.keep_original_track && p.audio.container === "mkv" ? "output.mkv" : "output.mp4";
      // Если имя всё же не совпало (редкий рассинхрон настроек), бэкенд сам резолвит выход через find_output — не гадаем здесь.
      api.reveal(pid, outName).catch(() => {});   // открыть проводник с выделенным готовым файлом — юзер видит, куда сохранилось
      pushActivity(`${t("compare.result")}: ${name}`, "done"); playSfx("success");
      setRendered(true); bumpDub();   // /dub now serves the freshly rendered output.mp4 -> reload <audio>
    } catch (err) {
      if (err instanceof JobCancelledError) {
        updateExport(exId, { status: "error", msg: t("jobs.state.cancelled") });
        pushActivity(t("jobs.cancelledKind", { kind: t("jobs.kind.render") }), "done");
      } else {
        updateExport(exId, { status: "error", msg: String(err) }); pushActivity(String(err), "error"); playSfx("error");
      }
    } finally { setRendering(false); }
  }
  async function doRemix() {                                            // Gemma rewrites the WHOLE script on a theme
    if (!remixText.trim() || remixing) return;
    setRemixing(true); pushActivity(t("remix.apply"));
    try {
      pushHistory(p);
      const { job_id } = await api.remix(pid, remixText.trim());
      await watchLocal(pid, "remix", job_id, (e) => { if (e.type === "progress") useStore.getState().setProgress(e.stage || "remix", e.msg || t("remix.apply"), e.pct ?? null); });
      setRendered(false);
      setProject(await api.getProject(pid));                            // rewritten transcript -> shows in the lane
      bump(); setLane("subs");                                          // показать переписанный текст сразу
      useStore.getState().setProgress("done", t("remix.apply"), null); // done-строка в журнале
    } catch (err) { if (err instanceof JobCancelledError) await jobCancelled("remix"); else await surfaceErr(err); }
    finally { setRemixing(false); }
  }

  const isActive = (seg: Project["segments"][number]) => scrub >= seg.start && scrub < seg.end;
  const activeId = p.segments.find(isActive)?.id;
  const audioOnly = !((p.meta.width || 0) > 0 && (p.meta.height || 0) > 0);   // вход без видео -> режим только аудио
  const mode = p.mode === "voiceover" ? "voiceover" : p.mode === "transcribe" ? "transcribe" : p.audio.rewrite ? "funny" : (p.mode === "nodub" ? "subtitles" : "dub");   // derived output mode
  const MODES = [["subtitles", Captions], ["dub", AudioLines], ["voiceover", Mic2], ["funny", Sparkles], ["transcribe", FileText]] as const;
  const cmds = [
    { label: t("mode.subtitles"), run: () => branch("mode", { value: "subtitles" }) },
    { label: t("mode.dub"), run: () => branch("mode", { value: "dub" }) },
    { label: t("mode.voiceover"), run: () => branch("mode", { value: "voiceover" }) },
    { label: t("mode.funny"), run: () => branch("mode", { value: "funny" }) },
    { label: t("mode.transcribe"), run: () => branch("mode", { value: "transcribe" }) },
    { label: t("export.proceed"), run: () => doExport() },
    { label: t("common.undo"), run: () => doUndo() },
    { label: t("common.redo"), run: () => doRedo() },
    { label: t("compare.toggle"), run: () => setCompare((c) => !c) },
    { label: t("hotkeys.title"), run: () => setShowHelp(true) },
    ...Object.keys(presets).map((n) => ({ label: `${t("preset.title")}: ${n}`, run: () => branch("preset", { name: n }) })),
  ];
  const activeRef = useRef<HTMLDivElement>(null);
  useEffect(() => { activeRef.current?.scrollIntoView({ block: "nearest", behavior: "smooth" }); }, [activeId]);
  // editor_* агента идут через те же функции, что и кнопки редактора.
  useEditorBridge({
    pid, mode, scrub, playing: play, lane, compare, castView, audioOnly, selSegs, busy: regenId !== null,
    seek: onSeek, playLine: playSeg,
    playFrom: (at) => { playEndRef.current = Infinity; if (audioRef.current) audioRef.current.currentTime = at; setScrub(at); setRendered(false); setPlay(true); },
    pause: () => setPlay(false),
    setLane: (next) => { setLane(next); setCastView(false); setCompare(false); },
    selectLines: setSelSegs, selectBlur: setSelBlur, selectTitle: setSelTitle,
    edit: applyEdit, undo: doUndo, redo: doRedo, exportVideo: doExport,
    openTakes: setTakesOpen, selectTake, pinTake, shortening: shortening !== null,
    shorten: (target) => { void doShorten(target, "ids" in target && target.ids.length === 1 ? target.ids[0] : "__all__"); },
  });
  useProjectSync(pid, () => draftOf(burstRef.current));
  useChanged(VOICES_CHANGED, () => {
    api.voices().then((r) => setVoiceList(r.voices)).catch((err) => pushActivity(t("bridge.syncFailed", { error: String(err) }), "error"));
  });
  useChanged(CASTING_CHANGED, (notice) => {
    if (notice.pid !== pid) return;
    api.casting(pid).then((r) => setCharacters(r.characters)).catch((err) => pushActivity(t("bridge.syncFailed", { error: String(err) }), "error"));
  });
  return (
    <div className="flex-1 grid grid-cols-[380px_1fr_300px] min-h-0">
      <aside className="border-r border-[var(--color-border)] flex flex-col min-h-0 overflow-hidden bg-[var(--color-surface)]">
        {/* Фикс-шапка: вкладки лейнов всегда видны (не скроллятся). */}
        <div className="shrink-0 px-4 pt-4 pb-2.5 border-b border-[var(--color-border)]">
        <div role="tablist" aria-label={t("a11y.lanes")} className="inline-flex rounded-lg bg-[var(--color-surface-2)] p-0.5 border border-[var(--color-border)] text-[12px]">
          {([["subs", t("mode.subtitles")], ["blur", `${t("blur.title")} ${(p.captions.blur_boxes || []).length}`],
            ["titles", `${t("titles.tab")} ${(p.captions.titles || []).length}`]] as const).map(([k, lbl]) => (
            <button key={k} onClick={() => setLane(k as typeof lane)} role="tab" aria-selected={lane === k}
              className={`px-2.5 py-1 rounded-md transition-colors ${lane === k ? "bg-[var(--color-accent)] text-[var(--color-on-accent)] font-semibold" : "text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
              {lbl}
            </button>
          ))}
          </div>
        </div>
        {/* Скролл-тело: скроллится только список, шапка выше зафиксирована. */}
        <div data-kb-scroll className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden px-4 pb-4">
        {lane === "subs" && (
        <div className="space-y-2">
          {(() => {
            const spks = [...new Set(p.segments.map((x) => x.speaker ?? "0"))].sort();
            const idsOf = (spk: string | null) => p.segments.filter((x) => spk === null || (x.speaker ?? "0") === spk).map((x) => x.id);
            const toggleMany = (ids: string[]) => setSelSegs((prev) => {
              const next = new Set(prev), all = ids.length > 0 && ids.every((i) => next.has(i));
              ids.forEach((i) => (all ? next.delete(i) : next.add(i)));
              return next;
            });
            const chip = "px-2 py-0.5 rounded-md text-[11px] border border-[var(--color-border)] bg-[var(--color-surface-2)] text-[var(--color-muted)] hover:text-[var(--color-text)] hover:border-[var(--color-accent)] transition-colors";
            return (
              <div className="sticky top-0 z-20 -mx-4 px-4 pt-1 pb-2 mb-1 space-y-1.5 bg-[var(--color-surface)] border-b border-[var(--color-border)]">
                <div className="flex flex-wrap items-center gap-1">
                  <span className="text-[10px] uppercase tracking-wider text-[var(--color-muted)] mr-0.5">{t("sel.pick")}</span>
                  <button onClick={() => toggleMany(idsOf(null))} className={chip}>{t("sel.all")}</button>
                  {spks.length > 1 && spks.map((spk) => <button key={spk} onClick={() => toggleMany(idsOf(spk))} className={chip}>SPK {spk}</button>)}

                  <div className="ml-auto inline-flex items-center gap-1.5 shrink-0">
                    <GlossaryButton pid={pid} project={p} />
                    <button onClick={handleSaveSubtitles} title={t("subs.saveTip")}
                      className="inline-flex items-center justify-center p-1 rounded-md bg-[var(--color-surface-2)] border border-[var(--color-border)] text-[var(--color-text)] hover:border-[var(--color-accent)] hover:text-[var(--color-accent)] transition-colors shrink-0">
                      <Save size={14} />
                    </button>
                    <label title={t("subs.importTip")} className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] bg-[var(--color-accent)] text-[var(--color-on-accent)] font-semibold cursor-pointer hover:brightness-110 transition shrink-0">
                      <Upload size={12} />
                      <span>{t("subs.importShort")}</span>
                      <input type="file" accept=".srt,.ass,.vtt,.sub,.txt" onChange={(e) => { const f = e.target.files?.[0]; if (f) handleImportSubtitles(f); e.target.value = ""; }} className="hidden" />
                    </label>
                  </div>
                </div>
                <FitToolbar segments={p.segments} onlyOver={onlyOver} onToggle={() => setOnlyOver((v) => !v)}
                  onShortenAll={() => doShorten({ all_over: true }, "__all__")} busy={shortening === "__all__"} disabled={regenId !== null || shortening !== null} />
                {selSegs.size > 0 && (
                  <div className="flex flex-wrap items-center gap-1.5 rounded-lg border border-[var(--color-accent)]/50 bg-[color-mix(in_oklab,var(--color-accent)_8%,transparent)] px-2 py-1.5">
                    <span className="text-[12px] font-medium mr-auto">{t("sel.count", { count: selSegs.size })}</span>
                    <button onClick={() => bulkSeg("keep_segments", { keep: true })} disabled={regenId !== null}
                      className="inline-flex items-center gap-1 px-2 py-1 rounded-md bg-[var(--color-surface-2)] text-[12px] hover:text-[var(--color-accent)] disabled:opacity-40 transition-colors"><Music size={13} />{t("sel.keep")}</button>
                    <button onClick={() => bulkSeg("hide_segments", { hidden: true })} disabled={regenId !== null}
                      className="inline-flex items-center gap-1 px-2 py-1 rounded-md bg-[var(--color-surface-2)] text-[12px] hover:text-[var(--color-accent)] disabled:opacity-40 transition-colors"><EyeOff size={13} />{t("sel.hide")}</button>
                    <button onClick={() => bulkSeg("del_segments")} disabled={regenId !== null}
                      className="inline-flex items-center gap-1 px-2 py-1 rounded-md bg-[var(--color-surface-2)] text-[12px] hover:text-[#ef4444] disabled:opacity-40 transition-colors"><Trash2 size={13} />{t("sel.del")}</button>
                    <button onClick={() => setSelSegs(new Set())} {...named(t("a11y.clearSelection"))} className="text-[var(--color-muted)] hover:text-[var(--color-text)]"><X size={14} /></button>
                  </div>
                )}
              </div>
            );
          })()}
          {p.segments.map((seg, idx) => {
            if (onlyOver && !fitOver(seg.fit)) return null;
            const on = isActive(seg);
            return (
              <div key={seg.id} ref={on ? activeRef : undefined} data-seg-id={seg.id}
                data-mcp-context={`segment ${seg.id} ${fmtT(seg.start)}→${fmtT(seg.end)} SPK ${seg.speaker ?? "-"}${seg.hidden ? " hidden" : ""}${seg.keep_original ? " original voice" : ""}${seg.dirty ? " dirty" : ""}: ${(seg.tgt_text || seg.src_text).slice(0, 60)}`}
                onDragOver={(e) => { e.preventDefault(); }}
                onDrop={(e) => { e.preventDefault(); dropSeg(seg.id); }}
                onClick={() => { setRendered(false); setScrub(seg.start); }}   // click a phrase -> seek the playhead to it
                className={`rounded-xl p-2 border-l-2 transition-colors cursor-pointer ${dragSegId === seg.id ? "opacity-30 border-dashed border-[var(--color-accent)]" : ""} ${seg.hidden ? "opacity-50" : ""} ${selSegs.has(seg.id) ? "ring-1 ring-[var(--color-accent)]/60" : ""} ${on ? "bg-[var(--color-surface-2)] border-[var(--color-accent)]" : "bg-[var(--color-surface-2)]/40 border-transparent hover:bg-[var(--color-surface-2)]/70"}`}>
                <div className="flex items-center justify-between gap-1">
                  <div className="flex items-center gap-1 flex-1 min-w-0">
                    <button type="button" draggable
                      onDragStart={(e) => { e.dataTransfer.setData("text/plain", seg.id); setDragSegId(seg.id); }}
                      onClick={(e) => e.stopPropagation()}
                      title={t("seg.drag")}
                      className="cursor-grab active:cursor-grabbing text-[var(--color-muted)] hover:text-[var(--color-accent)] p-0.5 rounded shrink-0">
                      <GripVertical size={13} />
                    </button>
                    <button onClick={(e) => { e.stopPropagation(); moveSeg(seg.id, "up"); }} disabled={idx === 0} title={t("seg.moveUp")}
                      className="p-0.5 text-[var(--color-muted)] hover:text-[var(--color-accent)] disabled:opacity-20 transition-colors shrink-0"><ChevronUp size={13} /></button>
                    <button onClick={(e) => { e.stopPropagation(); moveSeg(seg.id, "down"); }} disabled={idx === p.segments.length - 1} title={t("seg.moveDown")}
                      className="p-0.5 text-[var(--color-muted)] hover:text-[var(--color-accent)] disabled:opacity-20 transition-colors shrink-0"><ChevronDown size={13} /></button>
                    <button onClick={(e) => { e.stopPropagation(); setSelSegs((prev) => { const n = new Set(prev); n.has(seg.id) ? n.delete(seg.id) : n.add(seg.id); return n; }); }}
                      role="checkbox" aria-checked={selSegs.has(seg.id)} {...named(t("a11y.selectLine"))}
                      className={`grid place-items-center w-3.5 h-3.5 rounded shrink-0 border transition-colors ${selSegs.has(seg.id) ? "bg-[var(--color-accent)] border-[var(--color-accent)] text-[var(--color-on-accent)]" : "border-[var(--color-border)] hover:border-[var(--color-accent)]"}`}>
                      {selSegs.has(seg.id) && <Check size={10} />}</button>
                    {seg.speaker != null && <span className="mono px-1 py-0.5 rounded bg-[var(--color-overlay)] text-[9px] font-semibold text-[var(--color-muted)] shrink-0">SPK {seg.speaker}</span>}
                    <span className={`mono text-[9.5px] px-1 py-0.5 rounded tabnum shrink-0 ${on ? "bg-[var(--color-accent)] text-[var(--color-on-accent)] font-semibold" : "bg-[var(--color-overlay)] text-[var(--color-muted)]"}`}>{fmtT(seg.start)} → {fmtT(seg.end)}</span>
                    <FitBadge fit={seg.fit} />
                  </div>
                  <div className="flex items-center gap-0.5 shrink-0 bg-[var(--color-surface)] px-1 py-0.5 rounded-md border border-[var(--color-border)]/60">
                    {seg.dirty && <span className="text-[var(--color-accent)] text-[10px] mx-0.5" title={t("seg.edited")}>●</span>}
                    <button onClick={(e) => { e.stopPropagation(); playSeg(seg); }} title={t("seg.play")}
                      className="p-0.5 text-[var(--color-muted)] hover:text-[var(--color-accent)] transition-colors"><Play size={13} /></button>
                    {seg.fit && (
                      <button onClick={(e) => { e.stopPropagation(); doShorten({ ids: [seg.id] }, seg.id); }} title={t("fit.shortenTip")}
                        disabled={regenId !== null || shortening !== null || (!seg.fit.over && seg.fit.verdict === "fits")}
                        className="p-0.5 text-[var(--color-muted)] hover:text-[var(--color-accent)] disabled:opacity-40 transition-colors">
                        {shortening === seg.id ? <Loader2 size={13} className="animate-spin" /> : <Scissors size={13} />}
                      </button>
                    )}
                    {(seg.takes?.count ?? 0) > 0 && (
                      <button onClick={(e) => { e.stopPropagation(); setTakesOpen((cur) => (cur === seg.id ? null : seg.id)); }}
                        title={t("takes.toggle", { count: seg.takes?.count ?? 0 })} aria-pressed={takesOpen === seg.id}
                        className={`p-0.5 inline-flex items-center gap-0.5 transition-colors ${takesOpen === seg.id || seg.takes?.pinned != null ? "text-[var(--color-accent)]" : "text-[var(--color-muted)] hover:text-[var(--color-accent)]"}`}>
                        {seg.takes?.pinned != null ? <Pin size={12} /> : <History size={13} />}<span className="mono text-[9.5px] tabnum">{seg.takes?.count}</span>
                      </button>
                    )}
                    <button onClick={(e) => { e.stopPropagation(); doRegen(seg.id); }} disabled={regenId !== null} title={t("seg.regen")}
                      className="p-0.5 text-[var(--color-muted)] hover:text-[var(--color-accent)] disabled:opacity-40 transition-colors">
                      {regenId === seg.id ? <Loader2 size={13} className="animate-spin" /> : <RotateCw size={13} />}
                    </button>
                    <button onClick={(e) => { e.stopPropagation(); doKeepSeg(seg.id); }} disabled={regenId !== null} title={seg.keep_original ? t("seg.unkeep") : t("seg.keep")}
                      className={`p-0.5 disabled:opacity-40 transition-colors ${seg.keep_original ? "text-[var(--color-accent)]" : "text-[var(--color-muted)] hover:text-[var(--color-accent)]"}`}><Music size={13} /></button>
                    <button onClick={(e) => { e.stopPropagation(); doHideSeg(seg.id); }} disabled={regenId !== null} title={seg.hidden ? t("seg.show") : t("seg.hide")}
                      className="p-0.5 text-[var(--color-muted)] hover:text-[var(--color-accent)] disabled:opacity-40 transition-colors">
                      {seg.hidden ? <EyeOff size={13} /> : <Eye size={13} />}</button>
                    <button onClick={(e) => { e.stopPropagation(); doDelSeg(seg.id); }} disabled={regenId !== null} title={t("seg.del")}
                      className="p-0.5 text-[var(--color-muted)] hover:text-[#ef4444] disabled:opacity-40 transition-colors"><Trash2 size={13} /></button>
                  </div>
                </div>
                <SourceText pid={pid} seg={seg} />
                <AutoGrowTextarea value={seg.tgt_text} onChange={(e) => patchSeg(seg.id, e.target.value)}
                  onClick={(e) => e.stopPropagation()}                       // editing text must not re-seek on every click
                  onBlur={(e) => { burstRef.current = null; persistSeg(seg.id, e.target.value); }}   // end the edit burst
                  className="w-full mt-1.5 bg-[var(--color-bg)]/60 border border-[var(--color-border)] rounded-lg p-1.5 text-[13px] leading-snug resize-none overflow-hidden focus:border-[var(--color-accent)] focus:outline-none transition-colors" />
                {takesOpen === seg.id && (
                  <TakesPanel pid={pid} seg={seg} disabled={regenId !== null}
                    onSelect={(n) => doTakeSelect(seg.id, n)} onPin={(pinned) => doTakePin(seg.id, pinned)} />
                )}
                {seg.tts_skip && <TtsSkipNote reason={seg.tts_skip} />}
                {on && (
                  <div className="flex items-center gap-1.5 mt-1.5" onClick={(e) => e.stopPropagation()} title={t("seg.timingHint")}>
                    <Clock size={11} className="text-[var(--color-muted)] shrink-0" />
                    <input type="number" step={0.1} min={0} defaultValue={seg.start.toFixed(2)} key={`st${seg.id}-${seg.start}`}
                      onBlur={async (e) => { const v = parseFloat(e.target.value); if (!isNaN(v) && Math.abs(v - seg.start) > 0.001) { setRendered(false); try { setProject(await api.patch(pid, { op: "segment", id: seg.id, start: v })); bump(); } catch (err) { await surfaceErr(err); } } }}
                      className="w-[62px] bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded px-1.5 py-0.5 text-[11px] mono tabnum focus:border-[var(--color-accent)] focus:outline-none" />
                    <ArrowRight size={11} className="text-[var(--color-muted)] shrink-0" />
                    <input type="number" step={0.1} min={0} defaultValue={seg.end.toFixed(2)} key={`en${seg.id}-${seg.end}`}
                      onBlur={async (e) => { const v = parseFloat(e.target.value); if (!isNaN(v) && Math.abs(v - seg.end) > 0.001) { setRendered(false); try { setProject(await api.patch(pid, { op: "segment", id: seg.id, end: v })); bump(); } catch (err) { await surfaceErr(err); } } }}
                      className="w-[62px] bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded px-1.5 py-0.5 text-[11px] mono tabnum focus:border-[var(--color-accent)] focus:outline-none" />
                    <span className="text-[10px] text-[var(--color-muted)]">{t("seg.seconds")}</span>
                    <span className="flex-1" />
                    <button onClick={() => doSplit(seg)} disabled={regenId !== null || scrub <= seg.start + 0.1 || scrub >= seg.end - 0.1} {...named(t("montage.split"))}
                      className="p-0.5 text-[var(--color-muted)] hover:text-[var(--color-accent)] disabled:opacity-30 transition-colors"><Scissors size={13} /></button>
                    <button onClick={() => doMergeNext(seg)} disabled={regenId !== null || idx === p.segments.length - 1} {...named(t("montage.mergeNext"))}
                      className="p-0.5 text-[var(--color-muted)] hover:text-[var(--color-accent)] disabled:opacity-30 transition-colors"><Merge size={13} /></button>
                  </div>
                )}
                {(on || selSegs.has(seg.id)) && !seg.keep_original && (
                  <div className="flex items-center gap-1.5 mt-1.5" onClick={(e) => e.stopPropagation()} title={t("seg.speakerHint")}>
                    <Users size={11} className="text-[var(--color-muted)] shrink-0" />
                    <select value={seg.speaker ?? ""}
                      onChange={async (e) => {
                        let val = e.target.value;
                        if (val === "__new__") {                          // добавить НОВОГО спикера (ASR нашёл меньше, чем есть): следующий свободный id
                          const nums = p.segments.map((x) => parseInt(x.speaker ?? "", 10)).filter((n) => !isNaN(n));
                          val = String(nums.length ? Math.max(...nums) + 1 : 1);
                        }
                        setRendered(false);
                        try { setProject(await api.patch(pid, { op: "segment", id: seg.id, speaker: val })); bump(); } catch (err) { await surfaceErr(err); }
                      }}
                      className="flex-1 min-w-0 bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded px-1.5 py-0.5 text-[11px] focus:border-[var(--color-accent)] focus:outline-none transition-colors">
                      {seg.speaker == null && <option value="">—</option>}
                      {speakers.map((s) => <option key={s} value={s}>SPK {s}</option>)}
                      <option value="__new__">＋ {t("seg.newSpeaker")}</option>
                    </select>
                  </div>
                )}
              </div>
            );
          })}
          <button onClick={addSeg} disabled={regenId !== null} title={t("seg.addHint")}
            className="w-full mt-1 inline-flex items-center justify-center gap-1.5 py-2 rounded-xl border border-dashed border-[var(--color-border)] text-[12px] text-[var(--color-muted)] hover:text-[var(--color-accent)] hover:border-[var(--color-accent)] disabled:opacity-40 transition-colors">
            <Plus size={14} />{t("seg.add")}
          </button>
          <label title={t("subs.loadTip")}
            className="w-full mt-1.5 inline-flex items-center justify-center gap-1.5 py-2 rounded-xl border border-dashed border-[var(--color-border)] text-[12px] text-[var(--color-muted)] hover:text-[var(--color-accent)] hover:border-[var(--color-accent)] cursor-pointer transition-colors">
            <Upload size={14} /> {t("subs.importList")}
            <input type="file" accept=".srt,.ass,.vtt,.sub,.txt" onChange={(e) => { const f = e.target.files?.[0]; if (f) handleImportSubtitles(f); e.target.value = ""; }} className="hidden" />
          </label>
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
              {(p.captions.blur_boxes || []).length > 0 && (
                <div className="flex items-center gap-1.5 mb-1.5">
                  <button onClick={() => setSelBlurs((prev) => prev.size === (p.captions.blur_boxes || []).length ? new Set() : new Set((p.captions.blur_boxes || []).map((_, i) => i)))}
                    className="px-2 py-0.5 rounded-md text-[11px] border border-[var(--color-border)] bg-[var(--color-surface-2)] text-[var(--color-muted)] hover:text-[var(--color-text)] hover:border-[var(--color-accent)] transition-colors">{t("sel.all")}</button>
                  {selBlurs.size > 0 && (<>
                    <span className="text-[11px] text-[var(--color-muted)]">{t("sel.count", { count: selBlurs.size })}</span>
                    <button onClick={() => bulkDelIdx("del_blurs", selBlurs, () => setSelBlurs(new Set()))}
                      className="ml-auto inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] bg-[var(--color-surface-2)] hover:text-[#ef4444] transition-colors"><Trash2 size={12} />{t("sel.del")}</button>
                  </>)}
                </div>
              )}
              <div className="space-y-1 max-h-[46vh] overflow-y-auto pr-1">
                {(p.captions.blur_boxes || []).map((b, i) => ({ b, i }))
                  .filter(({ b }) => blurAll || (scrub >= b.t0 - 0.6 && scrub <= b.t1 + 0.4))
                  .map(({ b, i }) => (
                    <div key={i} onClick={() => { setSelBlur(i); setRendered(false); setScrub(Math.max(b.t0, 0)); }} data-blur-idx={i}
                      data-mcp-context={`blur ${i} ${b.w}x${b.h} at ${b.x},${b.y} ${fmtT(b.t0)}→${fmtT(b.t1)}${b.hidden ? " hidden" : ""}${b.fill ? ` fill ${b.fill}` : ""}`}
                      className={`flex items-center gap-2 mono text-[10px] rounded px-2 py-1 cursor-pointer transition-colors ${selBlur === i ? "bg-[color-mix(in_oklab,var(--color-accent)_18%,transparent)] text-[var(--color-text)] ring-1 ring-[var(--color-accent)]" : "text-[var(--color-muted)] bg-[var(--color-surface-2)]/40 hover:text-[var(--color-text)]"} ${b.hidden ? "opacity-50" : ""} ${selBlurs.has(i) ? "ring-1 ring-[var(--color-accent)]" : ""}`}>
                      <button onClick={(e) => { e.stopPropagation(); setSelBlurs((prev) => { const n = new Set(prev); n.has(i) ? n.delete(i) : n.add(i); return n; }); }}
                        role="checkbox" aria-checked={selBlurs.has(i)} {...named(t("a11y.selectBlur"))}
                        className={`grid place-items-center w-3.5 h-3.5 rounded-sm shrink-0 border transition-colors ${selBlurs.has(i) ? "bg-[var(--color-accent)] border-[var(--color-accent)] text-[var(--color-on-accent)]" : "border-[var(--color-border)] hover:border-[var(--color-accent)]"}`}>{selBlurs.has(i) && <Check size={9} />}</button>
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
                      <span className="inline-flex rounded border border-[var(--color-border)] overflow-hidden shrink-0">
                        <button onClick={(e) => { e.stopPropagation(); branch("blur", { idx: i, t0: Math.max(scrub, 0) }); }} title={t("edit.setStart")} className="px-1.5 py-1 text-[var(--color-muted)] hover:text-[var(--color-accent)] transition-colors"><ChevronFirst size={13} /></button>
                        <button onClick={(e) => { e.stopPropagation(); branch("blur", { idx: i, t1: scrub }); }} title={t("edit.setEnd")} className="px-1.5 py-1 text-[var(--color-muted)] hover:text-[var(--color-accent)] transition-colors"><ChevronLast size={13} /></button>
                        <button onClick={(e) => { e.stopPropagation(); branch("blur", { idx: i, t0: 0 }); }} title={t("edit.startVideo")} className="px-1.5 py-1 text-[var(--color-muted)] hover:text-[var(--color-accent)] transition-colors"><ArrowLeftToLine size={13} /></button>
                        <button onClick={(e) => { e.stopPropagation(); branch("blur", { idx: i, t1: p.meta.duration || 0 }); }} title={t("edit.endVideo")} className="px-1.5 py-1 text-[var(--color-muted)] hover:text-[var(--color-accent)] transition-colors"><ArrowRightToLine size={13} /></button>
                      </span>
                      <button onClick={(e) => { e.stopPropagation(); branch("blur_del", { idx: i }); setSelBlur(null); }} {...named(t("a11y.deleteBlur"))} className="shrink-0 hover:text-[var(--color-warn)] transition-colors"><Trash2 size={12} /></button>
                    </div>
                  ))}
                {!(p.captions.blur_boxes || []).some((b) => blurAll || (scrub >= b.t0 - 0.6 && scrub <= b.t1 + 0.4)) &&
                  <div className="text-[11px] text-[var(--color-muted)]/50 py-3 text-center">—</div>}
              </div>
              <button onClick={async () => { const fresh = await branch("blur_add", { x: Math.round((p.meta.width || 0) * 0.25), y: Math.round((p.meta.height || 0) * 0.45), w: Math.round((p.meta.width || 0) * 0.5), h: Math.round((p.meta.height || 0) * 0.08), t0: Math.max(0, scrub - 1), t1: scrub + 2 }); if (fresh) setSelBlur((fresh.captions.blur_boxes || []).length - 1); }}
                className="w-full mt-1.5 inline-flex items-center justify-center gap-1.5 text-[12px] py-1.5 rounded-lg border border-dashed border-[var(--color-border)] text-[var(--color-muted)] hover:border-[var(--color-accent)] hover:text-[var(--color-text)] transition-colors">
                <Plus size={13} /> {t("blur.add")}
              </button>
            </div>
          </div>
        )}
        {lane === "titles" && (
          <div className="space-y-2">
            {!(p.captions.titles || []).length && <div className="text-[11px] text-[var(--color-muted)]/50 py-3 text-center">—</div>}
            {(p.captions.titles || []).length > 0 && (
              <div className="flex items-center gap-1.5 mb-1">
                <button onClick={() => setSelTitles((prev) => prev.size === (p.captions.titles || []).length ? new Set() : new Set((p.captions.titles || []).map((_, i) => i)))}
                  className="px-2 py-0.5 rounded-md text-[11px] border border-[var(--color-border)] bg-[var(--color-surface-2)] text-[var(--color-muted)] hover:text-[var(--color-text)] hover:border-[var(--color-accent)] transition-colors">{t("sel.all")}</button>
                {selTitles.size > 0 && (<>
                  <span className="text-[11px] text-[var(--color-muted)]">{t("sel.count", { count: selTitles.size })}</span>
                  <button onClick={() => bulkDelIdx("del_titles", selTitles, () => setSelTitles(new Set()))}
                    className="ml-auto inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] bg-[var(--color-surface-2)] hover:text-[#ef4444] transition-colors"><Trash2 size={12} />{t("sel.del")}</button>
                </>)}
              </div>
            )}
            {(p.captions.titles || []).map((ti, i) => (
              <div key={`${ti.start}_${ti.end}_${i}`} onClick={() => { setSelTitle(i); setRendered(false); setScrub(Math.max(ti.start, 0)); }} data-title-idx={i}
                data-mcp-context={`title ${i} ${fmtT(ti.start)}→${fmtT(ti.end)}: ${(ti.tgt || ti.text).slice(0, 60)}`}
                className={`rounded-xl p-2.5 bg-[var(--color-surface-2)]/50 cursor-pointer transition-shadow ${selTitle === i ? "ring-1 ring-[var(--color-accent)]" : ""}`}>
                <div className="flex items-center gap-2 mono text-[10px] text-[var(--color-muted)] mb-1.5">
                  <button onClick={() => setSelTitles((prev) => { const n = new Set(prev); n.has(i) ? n.delete(i) : n.add(i); return n; })}
                    role="checkbox" aria-checked={selTitles.has(i)} {...named(t("a11y.selectTitle"))}
                    className={`grid place-items-center w-3.5 h-3.5 rounded-sm shrink-0 border transition-colors ${selTitles.has(i) ? "bg-[var(--color-accent)] border-[var(--color-accent)] text-[var(--color-on-accent)]" : "border-[var(--color-border)] hover:border-[var(--color-accent)]"}`}>{selTitles.has(i) && <Check size={9} />}</button>
                  <span className="tabnum">{fmtT(ti.start)} → {fmtT(ti.end)}</span>
                  <button onClick={(e) => { e.stopPropagation(); branch("title_del", { idx: i }); setSelTitle(null); }} className="ml-auto hover:text-[var(--color-warn)] transition-colors" title={t("titles.delete")}><Trash2 size={12} /></button>
                </div>
                <input value={ti.tgt || ti.text} onChange={(e) => titleText(i, e.target.value)} onClick={(e) => e.stopPropagation()}
                  onBlur={async (e) => { burstRef.current = null; setRendered(false); try { setProject(await api.patch(pid, { op: "title", idx: i, text: e.target.value, tgt: e.target.value })); bump(); } catch (err) { await surfaceErr(err); } }}
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
                    {SHADOW_DIRS.map(([v, l]) => <option key={v} value={v}>{l}</option>)}
                  </select>
                  <input key={`sz${i}-${ti.size_px ?? "a"}`} type="number" min={12} max={300} defaultValue={ti.size_px ?? undefined} placeholder="px" title={t("style.size")}
                    onBlur={(e) => branch("title", { idx: i, size_px: e.target.value ? parseInt(e.target.value) : null })}
                    className="w-12 bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded px-1 py-0.5 text-[11px] focus:border-[var(--color-accent)] focus:outline-none" />
                  <select value={ti.font || ""} onChange={(e) => branch("title", { idx: i, font: e.target.value })}
                    className="bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded px-1.5 py-0.5 text-[11px] focus:border-[var(--color-accent)] focus:outline-none">
                    <option value="">{t("style.font")}</option>
                    {Object.keys(fonts).map((f) => <option key={f} value={f}>{f}</option>)}
                  </select>
                  <span className="inline-flex rounded border border-[var(--color-border)] overflow-hidden">
                    {([["left", AlignLeft, "edit.alignLeft"], ["center", AlignCenter, "edit.alignCenter"], ["right", AlignRight, "edit.alignRight"]] as const).map(([a, Ic, k]) => (
                      <button key={a} onClick={(e) => { e.stopPropagation(); branch("title", { idx: i, align: a }); }} title={t(k)}
                        className={`px-1.5 py-1 transition-colors ${(ti.align || "center") === a ? "bg-[var(--color-accent)] text-[var(--color-on-accent)]" : "text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}><Ic size={12} /></button>
                    ))}
                  </span>
                  <span className="inline-flex rounded border border-[var(--color-border)] overflow-hidden shrink-0">
                    <button onClick={(e) => { e.stopPropagation(); branch("title", { idx: i, start: scrub }); }} title={t("edit.setStart")} className="px-1.5 py-1 text-[var(--color-muted)] hover:text-[var(--color-accent)] transition-colors"><ChevronFirst size={13} /></button>
                    <button onClick={(e) => { e.stopPropagation(); branch("title", { idx: i, end: scrub }); }} title={t("edit.setEnd")} className="px-1.5 py-1 text-[var(--color-muted)] hover:text-[var(--color-accent)] transition-colors"><ChevronLast size={13} /></button>
                    <button onClick={(e) => { e.stopPropagation(); branch("title", { idx: i, start: 0 }); }} title={t("edit.startVideo")} className="px-1.5 py-1 text-[var(--color-muted)] hover:text-[var(--color-accent)] transition-colors"><ArrowLeftToLine size={13} /></button>
                    <button onClick={(e) => { e.stopPropagation(); branch("title", { idx: i, end: p.meta.duration || 0 }); }} title={t("edit.endVideo")} className="px-1.5 py-1 text-[var(--color-muted)] hover:text-[var(--color-accent)] transition-colors"><ArrowRightToLine size={13} /></button>
                  </span>
                </div>
              </div>
            ))}
            <button onClick={async () => { const fresh = await branch("title_add", { text: t("titles.defaultText"), x: Math.round((p.meta.width || 0) * 0.15), y: Math.round((p.meta.height || 0) * 0.4), w: Math.round((p.meta.width || 0) * 0.7), h: Math.round((p.meta.height || 0) * 0.1), t0: Math.max(0, scrub - 0.5), t1: scrub + 3 }); if (fresh) setSelTitle((fresh.captions.titles || []).length - 1); }}
              className="w-full inline-flex items-center justify-center gap-1.5 text-[12px] py-1.5 rounded-lg border border-dashed border-[var(--color-border)] text-[var(--color-muted)] hover:border-[var(--color-accent)] hover:text-[var(--color-text)] transition-colors">
              <Plus size={13} /> {t("titles.add")}
            </button>
          </div>
        )}
        </div>
      </aside>

      <main className="flex flex-col min-w-0 min-h-0 overflow-hidden">
        <div className="flex items-center gap-3 px-4 py-2.5 border-b border-[var(--color-border)] bg-[var(--color-surface)]">
          {(() => { const s = document.getElementById("editor-modes-slot"); return s ? createPortal(
          <div className="inline-flex rounded-lg bg-[var(--color-surface-2)] p-0.5 border border-[var(--color-border)] shrink-0">
            {MODES.map(([k, Ic]) => (
              <button key={k} onClick={() => branch("mode", { value: k })} title={t(`mode.${k}_desc`)} data-mode={k} aria-pressed={mode === k}
                className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-[13px] transition-colors ${mode === k ? "bg-[var(--color-accent)] text-[var(--color-on-accent)] font-semibold" : "text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
                <Ic size={15} /> <span className="hidden xl:inline">{t(`mode.${k}`)}</span>
              </button>
            ))}
          </div>, s) : null; })()}
          {multiLangState.langs.length > 0 && (
            <button onClick={() => setStage("multilang")} title={t("multilang.backToExport")}
              className="shrink-0 inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md border border-[var(--color-border)] text-[12px] text-[var(--color-muted)] hover:text-[var(--color-accent)] hover:border-[var(--color-accent)] transition-colors">
              <Languages size={13} /><span className="hidden md:inline">{t("multilang.backToExport")}</span>
            </button>
          )}
          {/* КАСТИНГ (#115): вкладка «Персонажи» — видна, только если casting включался и персонажи найдены.
              Переключает главную область: сетка карточек ↔ превью. */}
          {hasCasting && (
            <button onClick={() => setCastView((v) => !v)} title={t("casting.tabHint")}
              className={`shrink-0 inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md border text-[12px] transition-colors ${castView ? "bg-[var(--color-accent)] text-[var(--color-on-accent)] border-transparent font-semibold" : "border-[var(--color-border)] text-[var(--color-muted)] hover:text-[var(--color-accent)] hover:border-[var(--color-accent)]"}`}>
              <Users size={13} /><span className="hidden md:inline">{t("casting.tab")}</span>
              <span className="mono text-[10px] opacity-70">{characters!.length}</span>
            </button>
          )}
          {!audioOnly && (
          <div className="flex items-center gap-1.5 shrink-0" title={t("comp.hint")}>
            <SubsContentControl subs={p.subs} primaryColor={ss.color || "#FFFFFF"} onPatch={branch} />
            <button onClick={() => branch("subs_burn", { on: p.subs.burn === false })} title={t("comp.burnHint")} aria-pressed={p.subs.burn !== false}
              className={`px-2.5 py-1 rounded-md text-[12px] border transition-colors ${p.subs.burn !== false ? "bg-[var(--color-accent)] text-[var(--color-on-accent)] border-transparent font-medium" : "border-[var(--color-border)] text-[var(--color-muted)]"}`}>
              {t("comp.burn")}
            </button>
          </div>
          )}
          <div className="flex-1" />
          <button onClick={doUndo} disabled={!canUndo} title={`${t("common.undo")} (Ctrl+Z)`}
            className="p-1.5 rounded-md text-[var(--color-muted)] hover:text-[var(--color-text)] disabled:opacity-30 transition-colors"><Undo2 size={16} /></button>
          <button onClick={doRedo} disabled={!canRedo} title={`${t("common.redo")} (Ctrl+Shift+Z)`}
            className="p-1.5 rounded-md text-[var(--color-muted)] hover:text-[var(--color-text)] disabled:opacity-30 transition-colors"><Redo2 size={16} /></button>
          {/* Экспорт-сплит: телепорт в TopBar (#editor-actions-slot). Основная кнопка — экспорт текущего; ▾ — «ещё языки». */}
          {(() => { const s = document.getElementById("editor-actions-slot"); return s ? createPortal(
          <div className="relative shrink-0 flex items-stretch">
            <button onClick={doExport} disabled={rendering}
              className={`inline-flex items-center gap-2 px-4 py-1.5 ${audioOnly ? "rounded-lg" : "rounded-l-lg"} bg-[var(--color-accent)] text-[var(--color-on-accent)] text-sm font-semibold disabled:opacity-70 hover:brightness-105 transition`}>
              {rendering ? <Loader2 size={16} className="animate-spin" /> : <Download size={16} />}{t("export.proceed")}
            </button>
            {!audioOnly && (
              <button onClick={() => setLangMenu((v) => !v)}
                title={t("multilang.exportHint")} disabled={rendering}
                className="inline-flex items-center px-1.5 rounded-r-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] border-l border-[color-mix(in_oklab,var(--color-on-accent)_30%,transparent)] disabled:opacity-70 hover:brightness-105 transition">
                <ChevronDown size={16} className={langMenu ? "rotate-180 transition-transform" : "transition-transform"} />
              </button>
            )}
            {langMenu && !audioOnly && (
              <div className="absolute right-0 top-full mt-2 z-30 w-72 rounded-xl border border-[var(--color-border)] bg-[var(--color-surface)] shadow-xl p-3">
                <div className="text-[12px] font-semibold mb-1">{t("multilang.exportTitle")}</div>
                <div className="text-[11px] text-[var(--color-muted)] mb-2 leading-snug">{t("multilang.exportHint")}</div>
                {exportLangs.length > 0 && (
                  <div className="flex flex-wrap gap-1.5 mb-2">
                    {exportLangs.map((code) => (
                      <span key={code} className="inline-flex items-center gap-1 pl-2 pr-1 py-0.5 rounded-md bg-[color-mix(in_oklab,var(--color-accent)_16%,transparent)] text-[var(--color-accent)] text-[11px]">
                        {DUB_LANGS.find((l) => l.code === code)?.name ?? code}
                        <button onClick={() => setExportLangs((xs) => xs.filter((x) => x !== code))} {...named(t("a11y.removeLang"))} className="hover:text-[var(--color-text)]"><X size={11} /></button>
                      </span>
                    ))}
                  </div>
                )}
                <Combobox value="" onChange={(c) => { if (c && !exportLangs.includes(c)) setExportLangs((xs) => [...xs, c]); }}
                  options={langOptions(DUB_LANGS.filter((l) => !exportLangs.includes(l.code) && l.code !== p.tgt_lang), i18n.language)}
                  placeholder={t("multilang.add")} noResults={t("voice.noMatch")} size="sm" className="w-full" />
                <button disabled={exportLangs.length === 0}
                  onClick={() => {
                    Object.assign(multiLangState, { sourcePid: pid, sourceName: p.meta.video || pid, langs: exportLangs });
                    setLangMenu(false); setStage("multilang");
                  }}
                  className="mt-2 w-full inline-flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-[12px] font-semibold disabled:opacity-50 hover:brightness-105">
                  <Languages size={13} />{t("multilang.exportGo", { n: exportLangs.length })}
                </button>
              </div>
            )}
          </div>, s) : null; })()}
        </div>
        <div ref={previewRef} className="fs-preview flex-1 min-h-0 p-3 overflow-hidden flex flex-col gap-2">
          <div className="flex-1 min-h-0">
          {/* #122/#115: НЕ размонтируем CastingPanel при переключении на превью — прячем через CSS, чтобы
              локальный черновик (имя/речь/голос) и imgFail пережили тоггл вкладки. Превью — в соседнем
              контейнере, скрытом при castView. Плеер дубляжа — отдельный <audio> в футере, display:none его не рвёт. */}
          {hasCasting && (
            <div className={castView ? "w-full h-full" : "hidden"}>
              <CastingPanel pid={pid} characters={characters!} voices={voiceList} onChange={setCharacters} />
            </div>
          )}
          <div className={hasCasting && castView ? "hidden" : "w-full h-full"}>
          {audioOnly ? (
            <div className="w-full h-full grid place-items-center rounded-xl bg-[var(--color-surface)] border border-[var(--color-border)]">
              <div className="text-center px-6">
                <AudioLines size={42} className="mx-auto text-[var(--color-accent)]" />
                <div className="mt-4 text-[16px] font-semibold">{t("audio.onlyTitle")}</div>
                <div className="mt-1.5 text-[13px] text-[var(--color-muted)] max-w-sm mx-auto leading-snug">{t("audio.onlyHint")}</div>
              </div>
            </div>
          ) : compare ? (
            <div className="w-full h-full grid grid-cols-2 gap-2 min-h-0">
              <ComparePane label={t("compare.original")} src={api.originalUrl(pid, scrub)} />
              <ComparePane label={t("compare.result")} src={api.previewUrl(pid, scrub, rev)} />
            </div>
          ) : (
            <PreviewCanvas pid={pid} project={p} scrub={scrub} rendered={rendered} lane={lane} playing={play}
              onChanged={(fresh) => setProject(fresh)} />
          )}
          </div>
          </div>
          {/* Ползунок перемотки + вейформа — СТРОГО друг под другом, ОДНОЙ ширины (полная ширина превью).
              Оба скрабят единый scrub. Время/транспорт — в баре ниже. */}
          {/* Интерактивный таймлайн снизу (в стиле ElevenLabs / NLE): визуализация плашек фраз + перетаскивание / растягивание */}
          {!audioOnly && (
            <div className="shrink-0 px-4 pb-1.5 flex flex-col gap-1.5">
              <input type="range" min={0} max={Math.max(0.1, p.meta.duration || 0)} step={0.05} value={Math.min(scrub, p.meta.duration || 0)}
                onChange={(e) => onSeek(parseFloat(e.target.value))}
                className="w-full h-1 accent-[var(--color-accent)] cursor-pointer" />
              <InteractiveTimeline pid={pid} duration={p.meta.duration || 0} scrub={scrub} segments={p.segments}
                playing={play} onSeek={onSeek} onPlaySeg={playSeg} setProject={setProject} />
            </div>
          )}
        </div>
        <div className="flex items-center gap-3 px-4 py-2.5 border-t border-[var(--color-border)] bg-[var(--color-surface)]">
          <button onClick={playFull} title={t("play.dub")} aria-pressed={play}
            className={`shrink-0 p-1.5 rounded-md transition-colors ${play ? "bg-[var(--color-accent)] text-[var(--color-on-accent)]" : "bg-[var(--color-surface-2)] text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
            {play ? <Pause size={15} /> : <Play size={15} />}
          </button>

          <audio ref={audioRef} src={api.dubUrl(pid, dubRev)} onEnded={() => setPlay(false)} preload="auto" className="hidden" />
          <div className="flex items-center gap-1.5 shrink-0" title={t("play.volume")}>
            <Music size={13} className="text-[var(--color-muted)]" />
            <input type="range" min={0} max={1} step={0.02} value={vol}
              onChange={(e) => { const v = parseFloat(e.target.value); setVol(v); if (audioRef.current) audioRef.current.volume = v; localStorage.setItem("dub-vol", String(v)); }}
              className="w-16 accent-[var(--color-accent)]" />
          </div>
          <button onClick={() => setCompare((c) => !c)} title={t("compare.toggle")} aria-pressed={compare}
            className={`shrink-0 p-1.5 rounded-md transition-colors ${compare ? "bg-[var(--color-accent)] text-[var(--color-on-accent)]" : "bg-[var(--color-surface-2)] text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
            <Columns2 size={15} />
          </button>
          <span className="mono text-[11px] tabnum shrink-0"><span className="text-[var(--color-accent)] font-semibold">{fmtT(scrub)}</span><span className="text-[var(--color-muted)]"> / {fmtT(p.meta.duration || 0)}</span></span>
          <div className="flex-1" />
        </div>
      </main>

      <aside data-kb-scroll data-style-panel="" className="border-l border-[var(--color-border)] overflow-y-auto p-4 bg-[var(--color-surface)] text-sm">
        <SectionLabel>{t("preset.title")}</SectionLabel>
        <div className="grid grid-cols-2 gap-1.5 mb-5 max-h-52 overflow-y-auto pr-1">
          <button onClick={() => branch("preset", { name: "" })} data-preset="" aria-pressed={!p.captions.preset?.name}
            className={`text-[11px] px-2 py-1.5 rounded-lg border transition-colors ${!p.captions.preset?.name ? "border-[var(--color-accent)] text-[var(--color-accent)] bg-[color-mix(in_oklab,var(--color-accent)_8%,transparent)]" : "border-[var(--color-border)] text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
            {t("preset.original")}
          </button>
          {Object.keys(presets).map((name) => (
            <button key={name} onClick={() => branch("preset", { name })} data-preset={name} aria-pressed={p.captions.preset?.name === name}
              className={`text-[11px] px-2 py-1.5 rounded-lg border truncate transition-colors ${p.captions.preset?.name === name ? "border-[var(--color-accent)] text-[var(--color-accent)] bg-[color-mix(in_oklab,var(--color-accent)_8%,transparent)]" : "border-[var(--color-border)] text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
              {name}
            </button>
          ))}
        </div>
        <SectionLabel>{t("editor.style")}</SectionLabel>
        <div className="space-y-3.5">
          {/* Шрифт */}
          <Row label={t("style.font")}>
            <select value={ss.font || "Montserrat"} onChange={(e) => branch("caption", { font: e.target.value })}
              className="bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-md px-2 py-1 text-[12px] max-w-[160px] focus:border-[var(--color-accent)] focus:outline-none transition-colors"
              title={fonts[ss.font || "Montserrat"] || ""}>
              {Object.keys(fonts).length ? Object.keys(fonts).map((f) => <option key={f} value={f}>{f}</option>)
                                         : <option value={ss.font || "Montserrat"}>{ss.font || "Montserrat"}</option>}
            </select>
          </Row>

          {/* Жирный, Курсив, CAPS, Выравнивание */}
          <div className="flex items-center gap-1.5 flex-wrap">
            <button onClick={() => branch("caption", { bold: !ss.bold })}
              className={`px-2 py-1 rounded-md text-[11px] font-bold border transition-colors ${ss.bold ? "border-[var(--color-accent)] text-[var(--color-accent)] bg-[color-mix(in_oklab,var(--color-accent)_10%,transparent)]" : "border-[var(--color-border)] text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
              {t("style.bold")}
            </button>
            <button onClick={() => branch("caption", { italic: !ss.italic })}
              className={`px-2 py-1 rounded-md text-[11px] italic border transition-colors ${ss.italic ? "border-[var(--color-accent)] text-[var(--color-accent)] bg-[color-mix(in_oklab,var(--color-accent)_10%,transparent)]" : "border-[var(--color-border)] text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
              {t("style.italic")}
            </button>
            <button onClick={() => branch("caption", { uppercase: !ss.uppercase })}
              className={`px-2 py-1 rounded-md text-[11px] font-semibold border transition-colors ${ss.uppercase ? "border-[var(--color-accent)] text-[var(--color-accent)] bg-[color-mix(in_oklab,var(--color-accent)_10%,transparent)]" : "border-[var(--color-border)] text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
              AA
            </button>
            <span className="inline-flex rounded-md border border-[var(--color-border)] overflow-hidden shrink-0 ml-auto">
              {([["left", AlignLeft, "edit.alignLeft"], ["center", AlignCenter, "edit.alignCenter"], ["right", AlignRight, "edit.alignRight"]] as const).map(([a, Ic, k]) => (
                <button key={a} onClick={() => branch("caption", { align: a })} title={t(k)}
                  className={`px-1.5 py-1 transition-colors ${(ss.align || "center") === a ? "bg-[var(--color-accent)] text-[var(--color-on-accent)]" : "text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}><Ic size={12} /></button>
              ))}
            </span>
          </div>

          {/* Цвет текста */}
          <Row label={t("style.color")}>
            <div className="flex items-center gap-2">
              <input type="color" value={ss.color || "#FFFFFF"} onChange={(e) => branch("caption", { color: e.target.value })}
                className="bg-transparent w-8 h-6 rounded cursor-pointer border border-[var(--color-border)]" />
              <span className="mono text-[11px] text-[var(--color-muted)]">{ss.color || "#FFFFFF"}</span>
            </div>
          </Row>

          {/* Обводка и тень */}
          <Row label={t("style.outline")}>
            <div className="flex items-center gap-1.5">
              <input type="color" value={ss.outline || "#000000"} onChange={(e) => branch("caption", { outline: e.target.value })}
                title={t("style.outline")} className="bg-transparent w-8 h-6 rounded cursor-pointer border border-dashed border-[var(--color-border)]" />
              <input key={`sow-${ss.outline_w ?? "a"}`} type="number" min={0} max={20} defaultValue={ss.outline_w ?? 2}
                placeholder={t("style.outlineW")} title={t("style.outlineWFull")}
                onBlur={(e) => branch("caption", { outline_w: e.target.value === "" ? null : parseInt(e.target.value) })}
                className="w-12 bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded px-1.5 py-0.5 text-[11px] focus:border-[var(--color-accent)] focus:outline-none" />
              <select value={ss.shadow_dir ?? ""} title={t("style.shadow")}
                onChange={(e) => branch("caption", { shadow_dir: e.target.value === "" ? null : parseInt(e.target.value) })}
                className="bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded px-1.5 py-0.5 text-[11px] focus:border-[var(--color-accent)] focus:outline-none">
                {SHADOW_DIRS.map(([v, l]) => <option key={v} value={v}>{l}</option>)}
              </select>
            </div>
          </Row>

          {/* Подложка (плашка) */}
          <Row label={t("style.plate")}>
            <div className="flex items-center gap-2">
              <Toggle label="" on={!!ss.plate} onClick={() => branch("caption", { plate: !ss.plate })} />
              {ss.plate && (
                <input type="color" value={ss.plate_color || "#000000"} onChange={(e) => branch("caption", { plate_color: e.target.value })}
                  title={t("style.plateColor")} className="bg-transparent w-7 h-5 rounded cursor-pointer border border-[var(--color-border)]" />
              )}
            </div>
          </Row>

          {/* Размер шрифта */}
          <div>
            <div className="flex items-center justify-between mb-1">
              <span className="text-[var(--color-muted)]">{t("style.size")}</span>
              <div className="flex items-center gap-1">
                <input type="number" min={12} max={300}
                  value={sizeDraft ?? ss.size_px ?? Math.round((p.meta.height || 1280) / 14)}
                  onChange={(e) => {
                    const val = parseInt(e.target.value);
                    if (!isNaN(val)) setSizeDraft(val);
                  }}
                  onBlur={async () => {
                    if (sizeDraft != null) { await branch("caption", { size_px: sizeDraft }); setSizeDraft(null); }
                  }}
                  className="w-12 bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded px-1 py-0.5 text-[11px] text-right font-mono focus:border-[var(--color-accent)] focus:outline-none" />
                <span className="mono text-[11px] text-[var(--color-muted)]">px</span>
              </div>
            </div>
            <input type="range" min={16} max={Math.round((p.meta.height || 1280) / 5)}
              value={sizeDraft ?? ss.size_px ?? Math.round((p.meta.height || 1280) / 14)}
              onChange={(e) => setSizeDraft(parseInt(e.target.value))}
              onPointerUp={async () => { if (sizeDraft != null) { await branch("caption", { size_px: sizeDraft }); setSizeDraft(null); } }}
              className="w-full accent-[var(--color-accent)] cursor-pointer" />
          </div>

          {/* Кнопка импорта субтитров в правой панели */}
          <div className="pt-3 border-t border-[var(--color-border)]/50 mt-4">
            <label className="w-full inline-flex items-center justify-center gap-2 px-3 py-2 rounded-xl bg-[var(--color-accent)] text-[var(--color-on-accent)] text-[12px] font-semibold cursor-pointer hover:brightness-105 transition shadow-sm">
              <Upload size={14} />
              <span>{t("subs.importPanel")}</span>
              <input type="file" accept=".srt,.ass,.vtt,.sub,.txt" onChange={(e) => { const f = e.target.files?.[0]; if (f) handleImportSubtitles(f); e.target.value = ""; }} className="hidden" />
            </label>
          </div>
        </div>
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
            <VoiceRecorder voiceList={voiceList} onVoices={setVoiceList}
              onDone={(nm) => branch("recast", { voice_mode: "voice", voice_name: nm })} />
            {(() => {
              const spks = [...new Set(p.segments.map((s) => s.speaker ?? "0"))].sort();
              if (spks.length === 0) return null;
              return (
                <div className="mt-2">
                  <div className="text-[10px] text-[var(--color-muted)] mb-1">{t("voice.fromSpeakers")}</div>
                  <div className="flex flex-wrap gap-1.5">
                    {spks.map((spk) => (
                      <button key={spk} disabled={spkVoiceBusy !== null}
                        onClick={async () => {
                          setSpkVoiceBusy(spk);
                          try { const r = await makeSpeakerVoice(pid!, spk, t("voice.speakerName", { spk })); if (r) { setVoiceList(r.voices); branch("recast", { voice_mode: "voice", voice_name: r.name }); } else playSfx("error"); } finally { setSpkVoiceBusy(null); }
                        }}
                        className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg border border-[var(--color-border)] text-[12px] hover:border-[var(--color-accent)] disabled:opacity-40">
                        {spkVoiceBusy === spk ? <Loader2 size={12} className="animate-spin" /> : <AudioLines size={12} className="text-[var(--color-accent)]" />}SPK {spk}
                      </button>
                    ))}
                  </div>
                </div>
              );
            })()}
            {/* Добавить спикера: если ASR нашёл меньше спикеров, чем есть. Выделенные галочками фразы (или
                текущую под плейхедом) назначаем новому SPK N — дальше ему можно задать свой голос выше. */}
            <button onClick={async () => {
              const nums = p.segments.map((x) => parseInt(x.speaker ?? "", 10)).filter((n) => !isNaN(n));
              const nid = String(nums.length ? Math.max(...nums) + 1 : 1);
              const targets = selSegs.size ? [...selSegs] : (() => { const act = p.segments.find((s) => scrub >= s.start && scrub <= s.end); return act ? [act.id] : []; })();
              if (!targets.length) { pushActivity(t("voice.addSpeakerPick"), "work"); return; }
              setRendered(false);
              try {
                let fresh = p;
                for (const id of targets) fresh = await api.patch(pid, { op: "segment", id, speaker: nid });
                setProject(fresh); bump(); setSelSegs(new Set());
                pushActivity(t("voice.addSpeakerDone", { spk: nid, count: targets.length }), "work");
              } catch (err) { await surfaceErr(err); }
            }}
              title={t("voice.addSpeakerHint")}
              className="mt-2 inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg border border-dashed border-[var(--color-border)] text-[12px] text-[var(--color-muted)] hover:text-[var(--color-accent)] hover:border-[var(--color-accent)] transition-colors">
              <Plus size={13} />{t("voice.addSpeaker")}
            </button>
            {p.audio.voice.mode !== "voice" && (
              <div className="mt-1 text-[10px] text-[var(--color-muted)] leading-snug">{t("voice.recordHint")}</div>
            )}
            {p.audio.voice.mode === "voice" && (() => {
              // per-speaker pack voices: engine maps a comma-list to sorted speakers (cycling), so each
              // diarized speaker can get a DISTINCT/funny voice. 1 speaker -> a single picker.
              const spks = [...new Set(p.segments.map((s) => s.speaker ?? "0"))].sort();   // lexical — matches engine sorted()
              const names = (p.audio.voice.name || "").split(",").map((s) => s.trim());
              const pick = (cur: string, on: (v: string) => void) => (
                <div className="flex items-center gap-1.5 min-w-0 flex-1">
                  <Combobox value={cur} onChange={on}
                    options={voiceList.map((v) => ({ value: v, label: v }))}
                    placeholder={voiceList.length ? t("voice.search") : t("voice.packMissing")}
                    noResults={t("voice.noMatch")} allowClear className="flex-1 min-w-0" />
                  <button type="button" disabled={!cur} onClick={() => toggleVoicePreview(cur)} title={t("voice.preview")}
                    className="shrink-0 w-8 h-8 inline-flex items-center justify-center rounded-lg border border-[var(--color-border)] hover:border-[var(--color-accent)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors">
                    {voicePreview === cur && cur ? <Pause size={14} className="text-[var(--color-accent)]" /> : <Play size={14} />}
                  </button>
                </div>
              );
              if (spks.length <= 1)
                return <div className="mt-2 flex w-full min-w-0">{pick(names[0] || "", (v) => branch("recast", { voice_mode: "voice", voice_name: v }))}</div>;
              return (
                <div className="mt-2 space-y-1.5 w-full min-w-0">
                  {spks.map((spk, i) => (
                    <div key={spk} className="flex items-center gap-2 w-full min-w-0">
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
            <div className="mt-3">
              <div className="flex items-center justify-between text-[11px] mb-1">
                <span className="text-[var(--color-muted)]">{t("voice.gain")}</span>
                <span className="mono text-[11px] text-[var(--color-text)]">{(gainDraft ?? p.audio.gain_db ?? 0) > 0 ? "+" : ""}{(gainDraft ?? p.audio.gain_db ?? 0).toFixed(1)} dB</span>
              </div>
              <input type="range" min={-12} max={12} step={0.5} value={gainDraft ?? p.audio.gain_db ?? 0}
                onChange={(e) => setGainDraft(parseFloat(e.target.value))}
                onPointerUp={async () => { if (gainDraft != null) { await doGain(gainDraft); setGainDraft(null); } }}
                className="w-full accent-[var(--color-accent)]" />
              <div className="text-[10px] text-[var(--color-muted)] leading-snug mt-0.5">{t("voice.gainHint")}</div>
            </div>
            <label className="mt-3 flex items-start gap-2 text-[11px] cursor-pointer">
              <input type="checkbox" checked={p.audio.loudness_normalize ?? true} onChange={(e) => void setLoudness(e.target.checked)}
                className="mt-0.5 accent-[var(--color-accent)]" />
              <span>
                <span className="text-[var(--color-text)]">{t("voice.loudness")}</span>
                <span className="block text-[10px] text-[var(--color-muted)] leading-snug mt-0.5">{t("voice.loudnessHint")}</span>
              </span>
            </label>
            {p.mode === "voiceover" && (   // закадровый: громкость ОРИГИНАЛЬНОЙ дорожки под переводом (0 = в полную силу, ниже = тише)
              <div className="mt-3">
                <div className="flex items-center justify-between text-[11px] mb-1">
                  <span className="text-[var(--color-muted)]">{t("voice.origGain")}</span>
                  <span className="mono text-[11px] text-[var(--color-text)]">{(voGainDraft ?? p.audio.voiceover_gain_db ?? -12).toFixed(1)} dB</span>
                </div>
                <input type="range" min={-24} max={0} step={0.5} value={voGainDraft ?? p.audio.voiceover_gain_db ?? -12}
                  onChange={(e) => setVoGainDraft(parseFloat(e.target.value))}
                  onPointerUp={async () => { if (voGainDraft != null) { await doVoiceoverGain(voGainDraft); setVoGainDraft(null); } }}
                  className="w-full accent-[var(--color-accent)]" />
                <div className="text-[10px] text-[var(--color-muted)] leading-snug mt-0.5">{t("voice.origGainHint")}</div>
              </div>
            )}
            <button onClick={doRegenAll} disabled={regenId !== null} title={t("voice.regenAll")}
              className="mt-3 w-full inline-flex items-center justify-center gap-2 px-3 py-2 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-sm font-semibold disabled:opacity-50 hover:brightness-105 transition">
              {regenId === "__all__" ? <Loader2 size={15} className="animate-spin" /> : <RotateCw size={15} />}{t("voice.regenAll")}
            </button>
          </>
        )}

      </aside>
      <CommandPalette commands={cmds} />
      {showHelp && <ShortcutsHelp onClose={() => setShowHelp(false)} />}
    </div>
  );
}

// Поп-ап доп-голосов (HF-датасет): поиск, фильтр по полу, прослушка (play mp3), скачка в каталог.
function VoicePackModal({ have, onVoices, onClose }: { have: string[]; onVoices: (v: string[]) => void; onClose: () => void }) {
  const { t } = useTranslation();
  const [all, setAll] = useState<{ name: string; gender: string; url: string }[] | null>(null);
  const [q, setQ] = useState("");
  const [gender, setGender] = useState<"all" | "female" | "male">("all");
  const [playing, setPlaying] = useState<string | null>(null);
  const [getting, setGetting] = useState<string | null>(null);
  const audio = useRef<HTMLAudioElement | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const reason = (e: unknown) => (e instanceof ApiError ? e.detail || e.code : e instanceof Error ? e.message : String(e));
  useEffect(() => {
    api.voicesCatalog().then((r) => setAll(r.voices), (e: unknown) => { setAll([]); setProblem(t("voice.catalogFailed", { error: reason(e) })); });
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => () => { audio.current?.pause(); }, []);

  const play = (url: string, name: string) => {
    audio.current?.pause();
    if (playing === name) { setPlaying(null); return; }
    const a = new Audio(url); audio.current = a; setPlaying(name);
    a.onended = () => setPlaying(null); a.play().catch(() => setPlaying(null));
  };
  const get = async (name: string) => {
    setGetting(name);
    setProblem(null);
    try {
      const r = await api.voicesGet(name);
      if (r.ok && r.voices) onVoices(r.voices);
      else setProblem(t("voice.getFailed", { name, error: r.error ?? "" }));
    } catch (e) {
      setProblem(t("voice.getFailed", { name, error: reason(e) }));
    } finally { setGetting(null); }
  };
  const pretty = (n: string) => n.replace(/^RU_(Female|Male)_/, "").replace(/_/g, " ");
  const filtered = (all || []).filter((v) =>
    (gender === "all" || v.gender === gender) &&
    (!q.trim() || pretty(v.name).toLowerCase().includes(q.trim().toLowerCase()))
  );

  return (
    <div className="fixed inset-0 z-[60] grid place-items-center glass-scrim anim-fade" onClick={onClose}>
      <div role="dialog" aria-modal="true" aria-labelledby="voicepack-title" className="w-[min(94vw,560px)] h-[min(82vh,640px)] flex flex-col rounded-xl glass-panel anim-pop p-5" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between mb-3">
          <span id="voicepack-title" className="font-semibold">{t("voice.packTitle")} {all && <span className="mono text-[11px] text-[var(--color-muted)]">{filtered.length}/{all.length}</span>}</span>
          <button onClick={onClose} {...named(t("a11y.close"))} className="text-[var(--color-muted)] hover:text-[var(--color-text)]"><X size={16} /></button>
        </div>
        <div className="flex items-center gap-2 mb-3">
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder={t("voice.search")} autoFocus
            className="flex-1 bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-lg px-2.5 py-1.5 text-[13px] focus:border-[var(--color-accent)] focus:outline-none" />
          <div className="flex rounded-lg border border-[var(--color-border)] overflow-hidden text-[12px]">
            {(["all", "female", "male"] as const).map((g) => (
              <button key={g} onClick={() => setGender(g)}
                className={`px-2.5 py-1.5 ${gender === g ? "bg-[var(--color-accent)] text-[var(--color-on-accent)]" : "text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>{t(`voice.g_${g}`)}</button>
            ))}
          </div>
        </div>
        {problem && <div role="alert" className="mb-2 text-[12px] text-[var(--color-warn)] break-words">{problem}</div>}
        <div className="flex-1 overflow-y-auto -mr-2 pr-2 space-y-1">
          {all === null ? <div className="mono text-[11px] text-[var(--color-muted)]">…</div> :
           filtered.slice(0, 300).map((v) => {
            const owned = have.includes(v.name) || have.includes(v.name.split("/").pop() || v.name);
            return (
              <div key={v.name} className="flex items-center gap-2 px-2.5 py-1.5 rounded-lg bg-[var(--color-surface-2)] border border-[var(--color-border)]">
                <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${v.gender === "female" ? "bg-pink-400" : "bg-sky-400"}`} />
                <span className="text-[12px] truncate flex-1">{pretty(v.name)}</span>
                <button onClick={() => play(v.url, v.name)} title={t("voice.preview")}
                  className="shrink-0 p-1 rounded-md text-[var(--color-muted)] hover:text-[var(--color-accent)]">{playing === v.name ? <Pause size={14} /> : <Play size={14} />}</button>
                {owned ? <Check size={14} className="text-[var(--color-accent)] shrink-0 w-7 text-center" /> :
                  <button onClick={() => get(v.name)} disabled={!!getting} title={t("setup.downloadOne")}
                    className="shrink-0 p-1 rounded-md text-[var(--color-accent-2)] hover:text-[var(--color-accent)] disabled:opacity-40">
                    {getting === v.name ? <Loader2 size={14} className="animate-spin" /> : <Download size={14} />}
                  </button>}
              </div>
            );
          })}
          {all !== null && filtered.length > 300 && <div className="mono text-[10px] text-[var(--color-muted)] text-center py-2">{t("voice.narrowSearch")}</div>}
        </div>
      </div>
    </div>
  );
}

// Языки с поиском по нативному имени + имени на языке интерфейса + английскому + коду (Intl.DisplayNames).
// На русском UI «французский»/«фра» найдёт Français, на английском — «french».
function langOptions(langs: { code: string; name: string }[], uiLang: string, extra: { value: string; label: string; search?: string }[] = []) {
  let dnUI: Intl.DisplayNames | null = null, dnEN: Intl.DisplayNames | null = null;
  try { dnUI = new Intl.DisplayNames([uiLang], { type: "language" }); } catch { /* локаль без поддержки */ }
  try { dnEN = new Intl.DisplayNames(["en"], { type: "language" }); } catch { /* */ }
  const nameIn = (dn: Intl.DisplayNames | null, code: string) => { try { return dn?.of(code) ?? ""; } catch { return ""; } };
  return [
    ...extra,
    ...langs.map((l) => ({ value: l.code, label: l.name, search: `${l.name} ${nameIn(dnUI, l.code)} ${nameIn(dnEN, l.code)} ${l.code}` })),
  ];
}

// Фильтруемый комбобокс: печатаешь кусок имени -> список сужается. Для больших списков (голоса/языки/шрифты).
function Combobox({ value, options, onChange, placeholder, noResults, allowClear, className, size = "md" }: {
  value: string;
  options: { value: string; label: string; search?: string }[];
  onChange: (v: string) => void;
  placeholder?: string;
  noResults?: string;
  allowClear?: boolean;               // пункт «—» для сброса в ""
  className?: string;
  size?: "sm" | "md";
}) {
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const boxRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => { if (boxRef.current && !boxRef.current.contains(e.target as Node)) { setOpen(false); setQ(""); } };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [open]);
  const ql = q.trim().toLowerCase();
  const filtered = ql ? options.filter((o) => `${o.label} ${o.value} ${o.search ?? ""}`.toLowerCase().includes(ql)) : options;
  const curLabel = options.find((o) => o.value === value)?.label ?? value;
  const commit = (v: string) => { onChange(v); setQ(""); setOpen(false); };
  const pad = size === "sm" ? "px-1.5 py-0.5 text-[11px]" : "px-2.5 py-1.5 text-[13px]";
  return (
    <div ref={boxRef} className={`relative ${className ?? ""}`}>
      <input
        value={open ? q : curLabel}
        placeholder={open ? (curLabel || placeholder) : placeholder}
        onFocus={() => { setOpen(true); setQ(""); }}
        onChange={(e) => { setQ(e.target.value); setOpen(true); }}
        onKeyDown={(e) => {
          if (e.key === "Enter" && filtered.length) { e.preventDefault(); commit(filtered[0].value); (e.target as HTMLInputElement).blur(); }
          else if (e.key === "Escape") { setOpen(false); setQ(""); (e.target as HTMLInputElement).blur(); }
        }}
        className={`w-full bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-lg ${pad} text-[var(--color-text)] focus:border-[var(--color-accent)] focus:outline-none transition-colors`} />
      {open && (
        <div className="absolute z-40 mt-1 w-full min-w-[180px] max-h-64 overflow-auto rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-2)] shadow-xl">
          {allowClear && (
            <button type="button" onMouseDown={(e) => e.preventDefault()} onClick={() => commit("")}
              className="block w-full text-left px-2.5 py-1.5 text-[12px] text-[var(--color-muted)] hover:bg-[var(--color-overlay)]">—</button>
          )}
          {filtered.length === 0 && <div className="px-2.5 py-2 text-[12px] text-[var(--color-muted)]">{noResults ?? "∅"}</div>}
          {filtered.map((o) => (
            <button key={o.value} type="button" onMouseDown={(e) => e.preventDefault()} onClick={() => commit(o.value)}
              title={o.label}
              className={`block w-full text-left px-2.5 py-1.5 text-[12px] truncate hover:bg-[var(--color-overlay)] ${o.value === value ? "text-[var(--color-accent)]" : "text-[var(--color-text)]"}`}>
              {o.label}</button>
          ))}
        </div>
      )}
    </div>
  );
}

// Запись своего голоса с микрофона (авто-нейминг) + доп-голоса. onDone(name) — новый голос готов.
function VoiceRecorder({ voiceList, onVoices, onDone }: { voiceList: string[]; onVoices: (v: string[]) => void; onDone: (name: string) => void }) {
  const { t } = useTranslation();
  const [rec, setRec] = useState(false);
  const [level, setLevel] = useState(0);
  const [pack, setPack] = useState(false);
  const [rename, setRename] = useState<{ from: string; to: string } | null>(null);
  const lvlTimer = useRef<number | null>(null);

  const start = async () => {
    const auto = t("voice.autoName", { n: voiceList.length + 1 }); // авто-имя, переименовать можно после
    const r = await api.recordStart(auto).catch(() => null);
    if (!r || !r.ok) return;
    setRec(true);
    lvlTimer.current = window.setInterval(async () => {
      try { const l = await api.recordLevel(); setLevel(l.level); } catch { /* ignore */ }
    }, 100);
  };
  const stop = async () => {
    if (lvlTimer.current) window.clearInterval(lvlTimer.current);
    setRec(false); setLevel(0);
    const r = await api.recordStop().catch(() => null);
    if (r) { onVoices(r.voices); if (r.name) { onDone(r.name); setRename({ from: r.name, to: r.name }); } }
  };
  const applyRename = async () => {
    if (!rename || !rename.to.trim() || rename.to === rename.from) { setRename(null); return; }
    const r = await api.voicesRename(rename.from, rename.to).catch(() => null);
    if (r) { onVoices(r.voices); onDone(rename.to.trim()); }
    setRename(null);
  };

  return (
    <div className="mt-2 space-y-2">
      <div className="flex items-center gap-2">
        {rec ? (
          <button onClick={stop} className="flex-1 inline-flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg bg-[var(--color-danger,#ef4444)] text-white text-[12px] font-semibold">
            <Square size={13} />{t("voice.recordStop")}
          </button>
        ) : (
          <button onClick={start} title={t("voice.record")} className="flex-1 inline-flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg border border-[var(--color-border)] text-[12px] font-medium hover:border-[var(--color-accent)]">
            <span className="w-2 h-2 rounded-full bg-[var(--color-danger,#ef4444)]" />{t("voice.record")}
          </button>
        )}
        <button onClick={() => setPack(true)} title={t("voice.packTitle")}
          className="shrink-0 inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-[var(--color-border)] text-[12px] text-[var(--color-muted)] hover:border-[var(--color-accent)] hover:text-[var(--color-text)]">
          <Download size={13} />{t("voice.more")}
        </button>
      </div>
      {rec && (
        <div className="h-1.5 rounded-full bg-white/10 overflow-hidden">
          <div className="h-full bg-[var(--color-accent)] transition-[width] duration-100" style={{ width: `${Math.min(100, level * 140)}%` }} />
        </div>
      )}
      {rename && !rec && (
        <div className="flex items-center gap-2">
          <input value={rename.to} onChange={(e) => setRename({ ...rename, to: e.target.value })} autoFocus
            onKeyDown={(e) => { if (e.key === "Enter") applyRename(); }}
            placeholder={t("voice.recordName")}
            className="flex-1 min-w-0 bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-lg px-2.5 py-1.5 text-[12px] focus:border-[var(--color-accent)] focus:outline-none" />
          <button onClick={applyRename} className="shrink-0 px-2.5 py-1.5 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-[12px] font-semibold"><Check size={13} /></button>
        </div>
      )}
      {pack && <VoicePackModal have={voiceList} onVoices={onVoices} onClose={() => setPack(false)} />}
    </div>
  );
}

// Non-blocking export queue: a floating Files panel (bottom-right). You keep editing while renders run; each
// finished file gets Download + Open. Subscribes only to `exports`, so it repaints independently of the editor.
function FilesPanel() {
  const { t } = useTranslation();
  const exports = useStore((s) => s.exports);
  const [, force] = useState(0);
  const [dled, setDled] = useState<string | null>(null);       // id только что скачанного -> показать «Сохранено» (обратная связь)
  const [opening, setOpening] = useState<string | null>(null); // id открываемого файла
  const fl = useFloatable("files", { x: window.innerWidth - 360, y: 64 });
  const prevLen = useRef(0);
  useEffect(() => { force((n) => n + 1); }, []); // дождаться #dock-slot
  useEffect(() => { // первый экспорт -> раскрыть панель во float
    if (prevLen.current === 0 && exports.length > 0 && !fl.floating) fl.pop();
    prevLen.current = exports.length;
  }, [exports.length]); // eslint-disable-line react-hooks/exhaustive-deps
  if (!exports.length) return null;
  const active = exports.filter((e) => e.status === "rendering").length;

  // докнутый чип в шапке
  if (!fl.floating) {
    const slot = dockSlot();
    if (!slot) return null;
    return createPortal(
      <button onClick={fl.pop} title={t("files.title")}
        className="inline-flex items-center gap-2 px-2.5 h-8 rounded-md bg-[var(--color-surface-2)] border border-[var(--color-border)] hover:border-[var(--color-accent)] transition-colors">
        {active ? <Loader2 size={14} className="animate-spin text-[var(--color-accent)]" /> : <FolderDown size={14} className="text-[var(--color-accent)]" />}
        <span className="text-[12px] font-medium hidden sm:inline">{t("files.title")}</span>
        <span className="mono text-[11px] text-[var(--color-muted)]">{exports.length}</span>
      </button>,
      slot,
    );
  }

  // оторванная драг-панель
  return (
    <div className="fixed z-50 w-[min(90vw,340px)]" style={{ left: fl.pos.x, top: fl.pos.y }}>
      {(
        <div className="w-full rounded-xl glass-panel anim-pop overflow-hidden">
          <div onPointerDown={fl.onDragStart}
            className={`flex items-center justify-between px-3 py-2 border-b border-[var(--color-border)] ${fl.dragging ? "cursor-grabbing" : "cursor-grab"}`}>
            <span className="flex items-center gap-1.5 mono text-[11px] uppercase tracking-[0.14em] text-[var(--color-muted)]"><Move size={12} />{t("files.title")}</span>
            <button onClick={fl.dock} title={t("float.dock")} className="text-[var(--color-muted)] hover:text-[var(--color-text)]"><Minimize2 size={14} /></button>
          </div>
          <div className="max-h-[52vh] overflow-y-auto p-2 space-y-1.5">
            {exports.map((e) => (
              <div key={e.id} data-mcp-context={`export ${e.name}: ${e.status}`} className="rounded-lg bg-[var(--color-surface-2)]/60 p-2.5">
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
                    <a href={`${e.url}&dl=1`} download
                       onClick={() => { setDled(e.id); window.setTimeout(() => setDled((c) => (c === e.id ? null : c)), 2500); }}
                       className="flex-1 inline-flex items-center justify-center gap-1.5 text-[12px] py-1 rounded-md bg-[var(--color-accent)] text-[var(--color-on-accent)] font-semibold">
                      {dled === e.id ? <><Check size={13} /> {t("files.downloaded")}</> : <><Download size={13} /> {t("files.download")}</>}
                    </a>
                    <button onClick={async () => { if (!e.pid) return; setOpening(e.id); try { await api.openOutput(e.pid); } catch { /* ignore */ } finally { window.setTimeout(() => setOpening((c) => (c === e.id ? null : c)), 800); } }}
                       className="inline-flex items-center justify-center gap-1.5 text-[12px] px-2.5 py-1 rounded-md border border-[var(--color-border)] text-[var(--color-muted)] hover:text-[var(--color-text)]">
                      {opening === e.id ? <Loader2 size={13} className="animate-spin" /> : <ExternalLink size={13} />} {t("files.open")}
                    </button>
                  </div>
                )}
                {e.status === "error" && <div className="mt-1 mono text-[10px] text-[var(--color-warn)] truncate">{e.msg}</div>}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

// Семейства квантов: разные варианты ОДНОЙ модели (выбор одного, «ИЛИ»). Явная карта по id — надёжнее
// префикса (higgs — квант TTS, higgs-engine — движок, это РАЗНЫЕ вещи). Остальные компоненты — одиночные.
const QUANT_GROUP: Record<string, string> = {
  higgs: "higgs", "higgs-q6_k": "higgs", "higgs-q4_k_m": "higgs",
  gemma: "gemma", "gemma-q5_0": "gemma", "gemma-q6_k": "gemma", "gemma-q8_0": "gemma",
  parakeet: "parakeet", "parakeet-fp32": "parakeet", "parakeet-ultra": "parakeet", "parakeet-ultra-int8": "parakeet",
  roformer: "roformer", "roformer-q5": "roformer", "roformer-q4": "roformer",
};

// ── «Первый запуск»: панель автозакачки компонентов (модели/движки/системные библиотеки) ──
// По умолчанию отмечено всё недостающее скачиваемое, кроме опциональных квантов (их качают вручную).
const preselect = (s: SetupStatus) =>
  new Set(s.components.filter((c) => c.delivery === "download" && !c.installed && c.requirement !== "optional").map((c) => c.id));

function FirstRun() {
  const { t } = useTranslation();
  // Галочка статистики «Первого запуска»: включена, уходит с первой закачкой или «Продолжить» (выбор увиден).
  const [telemetryOn, setTelemetryOn] = useState(true);
  const [telemetryPreview, setTelemetryPreview] = useState(false);
  const sendTelemetryChoice = () => setTelemetry(telemetryOn, true).catch((e: Error) => console.warn("[hub] the statistics choice was not saved:", e.message));
  const errText = useDownloadErrorText();
  const setStage = useStore((s) => s.setStage);
  // Выбор пользователя; null — ещё не трогал, берём преселект из текущего статуса (опрос его не сбрасывает).
  const [picked, setPicked] = useState<Set<string> | null>(null);
  const [err, setErr] = useState<string | null>(null);
  // Закачка идёт фоном на сервере: прогресс — опросом статуса, переживает перезагрузку окна и приложения.
  const onSettled = (job: DownloadJob, s: SetupStatus) => {
    if (job.status === "completed") useStore.getState().pushActivity(t(s.ready ? "setup.ready" : "setup.done"), "done");
    if (job.status === "completed" && s.ready) { setStage("empty"); playSfx("success"); }   // всё скачано -> сразу на стартовый экран
    else if (job.status === "failed") useStore.getState().pushActivity(`${errText(job.errorCode)} · ${job.error ?? ""}`, "error");
  };
  const { status, error: statusErr, refresh, adopt, downloading: busy } = useSetupStatus(onSettled);
  const sel = picked ?? (status ? preselect(status) : new Set<string>());
  const setSel = (f: (prev: Set<string>) => Set<string>) => setPicked(f(sel));
  const active = status?.active ?? null;
  const partPct = (id: string): number | null => {
    if (!busy || !active) return null;
    const p = active.parts.find((x) => x.id === id);
    return p && p.total > 0 ? (p.done / p.total) * 100 : active.ids.includes(id) ? 0 : null;
  };

  async function download(ids: string[]) {
    if (ids.length === 0 || busy) return;
    setErr(null);
    try {
      await api.setupDownload(ids);
      await refresh();
    } catch (e) {
      const msg = e instanceof SetupError ? `${errText(e.code)} · ${e.detail}` : String(e);
      setErr(msg); useStore.getState().pushActivity(msg, "error");
    }
  }
  const pause = () => { api.setupCancel().then(() => refresh()).catch((e) => setErr(String(e))); };

  const toggle = (id: string) => setSel((prev) => { const n = new Set(prev); if (n.has(id)) n.delete(id); else n.add(id); return n; });
  const selectedComps = status ? status.components.filter((c) => sel.has(c.id)) : [];
  const selectedBytes = selectedComps.reduce((a, c) => a + Math.max(0, c.size - c.bytesOnDisk), 0);
  const selectedSpace = selectedComps.reduce((a, c) => a + c.spaceNeeded, 0);
  const noSpace = status?.freeBytes != null && selectedSpace > status.freeBytes;

  const reqLabel = (r: string) => (r === "required" ? t("setup.required") : t("setup.recommended"));
  const deliveryNote = (c: SetupComponent) =>
    c.delivery === "bundled" ? t("setup.reinstallHint") : c.delivery === "external" ? t("setup.external") : "";
  const GROUP_LABEL: Record<string, string> = { higgs: t("setup.grpHiggs"), gemma: t("setup.grpGemma"), parakeet: t("setup.grpParakeet"), roformer: t("setup.grpRoformer") };
  const pickOne = (id: string, group: string) => setSel((prev) => {   // radio внутри семейства: выбрать этот квант, снять остальные того же семейства
    const n = new Set(prev);
    Object.entries(QUANT_GROUP).forEach(([cid, g]) => { if (g === group) n.delete(cid); });
    n.add(id);
    return n;
  });
  const comps = status?.components ?? [];
  const gseen = new Set<string>();
  type SetupRow = { kind: "one"; c: SetupComponent } | { kind: "group"; group: string; members: SetupComponent[] };
  const rows: SetupRow[] = comps.flatMap((c): SetupRow[] => {   // одиночные компоненты + по одной группе квантов (на первом её члене)
    const g = QUANT_GROUP[c.id];
    if (!g) return [{ kind: "one", c }];
    if (gseen.has(g)) return [];
    gseen.add(g);
    return [{ kind: "group", group: g, members: comps.filter((x) => QUANT_GROUP[x.id] === g) }];
  });

  return (
    <div className="flex-1 min-h-0 overflow-y-auto grid place-items-center px-6 py-10">
      <motion.div initial={{ opacity: 0, y: 14 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.5, ease: EASE }}
        className="w-full max-w-2xl">
        <div className="mono text-[11px] tracking-[0.2em] text-[var(--color-muted)] flex items-center gap-2">
          <span className="w-1.5 h-1.5 rounded-full bg-[var(--color-accent)] shadow-[0_0_8px_var(--color-accent)]" />{t("setup.title")}
        </div>
        <h1 className="mt-4 text-2xl lg:text-[28px] leading-tight font-extrabold tracking-tight">{t("setup.title")}</h1>
        <p className="mt-3 text-[14px] leading-relaxed text-[var(--color-muted)]">{t("setup.subtitle")}</p>

        {err && (
          <div className="mt-4 rounded-lg border border-[var(--color-warn)]/40 bg-[color-mix(in_oklab,var(--color-warn)_10%,transparent)] px-3 py-2 text-[13px] text-[var(--color-warn)]">
            <span className="font-semibold">{t("setup.error")}</span> · <span className="mono text-[11px] break-words">{err}</span>
          </div>
        )}

        {statusErr && (
          <div className="mt-4 rounded-lg border border-[var(--color-warn)]/40 px-3 py-2 mono text-[11px] text-[var(--color-warn)] break-words">{statusErr}</div>
        )}

        {status && (
          <div className="mt-4 space-y-2">
            <GpuNotice gpu={status.gpu} driverUrl={status.components.find((c) => c.delivery === "external")?.externalUrl} />
            {active && active.status !== "completed" && <DownloadProgress job={active} onPause={pause} onResume={(ids) => { download(ids); }} />}
            <ModelsFolder status={status} need={selectedSpace} />
          </div>
        )}

        <div className="mt-4 rounded-lg border border-[var(--color-accent)]/30 bg-[color-mix(in_oklab,var(--color-accent)_8%,transparent)] px-3.5 py-2.5 text-[12.5px] leading-snug text-[var(--color-text)]">
          <span className="font-semibold text-[var(--color-accent)]">{t("setup.pickNoteTitle")}</span> {t("setup.pickNote")}
        </div>

        {/* Пресет под железо + ключ OpenRouter: выбрал «Облако» → облачные движки, локальные модели ниже
            становятся необязательными (не тянешь лишние гигабайты). Refresh обновляет чеклист под выбор. */}
        <div className="mt-4 rounded-xl border border-[var(--color-border)] bg-[var(--color-surface)] px-3.5 py-3 space-y-2">
          <PresetsSection onApplied={() => { setPicked(null); refresh().catch(() => {}); }} />
          <OpenRouterKey onSaved={() => { setPicked(null); refresh().catch(() => {}); }} />
          <div className="text-[11px] text-[var(--color-muted)] leading-snug"><Trans i18nKey="setup.cloudHint" components={{ b: <span className="text-[var(--color-text)]" /> }} /></div>
        </div>

        <div className="mt-4 space-y-2">
          {rows.map((row) => {
            if (row.kind === "group") {
              const { group, members } = row;
              return (
                <div key={group} className="rounded-xl border border-[var(--color-border)] bg-[var(--color-surface)] px-3.5 py-3">
                  <div className="flex items-center justify-between mb-2">
                    <span className="text-[13px] font-semibold">{GROUP_LABEL[group]}</span>
                    <span className="text-[10px] uppercase tracking-wide px-1.5 py-0.5 rounded bg-[color-mix(in_oklab,var(--color-accent)_16%,transparent)] text-[var(--color-accent)]">{t("setup.pickOneQuant")}</span>
                  </div>
                  <div className="space-y-0.5">
                    {members.map((c) => {
                      const isDefault = c.requirement !== "optional";
                      const checked = sel.has(c.id);
                      const pct = partPct(c.id);
                      return (
                        <label key={c.id} className={`flex items-center gap-2.5 rounded-lg px-2 py-1.5 ${c.installed ? "" : "cursor-pointer hover:bg-[var(--color-surface-2)]"} ${checked && !c.installed ? "bg-[color-mix(in_oklab,var(--color-accent)_8%,transparent)]" : ""}`}>
                          {c.installed
                            ? <span className="w-4 h-4 shrink-0 grid place-items-center text-[var(--color-accent)]"><Check size={15} strokeWidth={3} /></span>
                            : <input type="radio" name={`grp-${group}`} checked={checked} onChange={() => pickOne(c.id, group)} disabled={busy} className="w-4 h-4 accent-[var(--color-accent)] shrink-0" />}
                          <div className="min-w-0 flex-1">
                            <div className="flex items-center gap-2">
                              <span className="text-[13px] truncate">{c.name}</span>
                              {isDefault
                                ? <span className="text-[9px] px-1.5 py-0.5 rounded bg-[color-mix(in_oklab,var(--color-accent)_16%,transparent)] text-[var(--color-accent)] uppercase tracking-wide shrink-0">{t("setup.default")}</span>
                                : <span className="text-[9px] px-1.5 py-0.5 rounded bg-[var(--color-surface-2)] text-[var(--color-muted)] uppercase tracking-wide shrink-0">{t("setup.alt")}</span>}
                              {c.installed && <span className="text-[10px] text-[var(--color-accent)] shrink-0">{t("setup.installed")}</span>}
                            </div>
                            {ASR_VARIANT_I18N[c.id] && <div className="text-[11px] leading-snug text-[var(--color-muted)]">{t(ASR_VARIANT_I18N[c.id].hint)}</div>}
                            {pct != null && <div className="mt-1 h-1 rounded-full bg-[var(--color-surface-2)] overflow-hidden"><div className="h-full bg-[var(--color-accent)]" style={{ width: `${pct}%` }} /></div>}
                          </div>
                          <span className="mono text-[11px] text-[var(--color-muted)] shrink-0">{fmtBytes(c.size)}{c.vram ? ` · ${fmtBytes(c.vram)} VRAM` : ""}</span>
                        </label>
                      );
                    })}
                  </div>
                  <div className="mt-1.5 text-[10px] text-[var(--color-muted)]">{t("setup.quantHint")}</div>
                </div>
              );
            }
            const c = row.c;
            const pct = partPct(c.id);
            const canPick = c.delivery === "download" && !c.installed;
            return (
              <div key={c.id} className="rounded-xl border border-[var(--color-border)] bg-[var(--color-surface)] px-3.5 py-3">
                <div className="flex items-center gap-3">
                  {canPick ? (
                    <input type="checkbox" checked={sel.has(c.id)} onChange={() => toggle(c.id)} disabled={busy}
                      className="w-4 h-4 accent-[var(--color-accent)] shrink-0" />
                  ) : (
                    <span className={`w-4 h-4 shrink-0 grid place-items-center rounded ${c.installed ? "text-[var(--color-accent)]" : "text-[var(--color-muted)]"}`}>
                      {c.installed ? <Check size={16} strokeWidth={3} /> : <X size={14} />}
                    </span>
                  )}
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="font-semibold text-[14px] truncate">{c.name}</span>
                      <span className={`text-[10px] px-1.5 py-0.5 rounded uppercase tracking-wide ${c.requirement === "required" ? "bg-[color-mix(in_oklab,var(--color-accent)_16%,transparent)] text-[var(--color-accent)]" : "bg-[var(--color-surface-2)] text-[var(--color-muted)]"}`}>{reqLabel(c.requirement)}</span>
                    </div>
                    <div className="text-[12px] text-[var(--color-muted)] truncate">{c.purpose}</div>
                    {c.delivery === "external" && c.detail && <div className="mono text-[10.5px] text-[var(--color-muted)] truncate">{c.detail}</div>}
                    {pct != null && (
                      <div className="mt-1.5 h-1.5 rounded-full bg-[var(--color-surface-2)] overflow-hidden">
                        <div className="h-full bg-[var(--color-accent)] transition-[width] duration-200" style={{ width: `${pct}%` }} />
                      </div>
                    )}
                  </div>
                  <div className="text-right shrink-0">
                    {c.delivery === "external" ? (
                      c.installed ? <span className="text-[12px] text-[var(--color-accent)]">{t("setup.installed")}</span>
                        : <button onClick={() => c.externalUrl && window.open(c.externalUrl, "_blank")}
                            className="inline-flex items-center gap-1 text-[12px] text-[var(--color-accent-2)] hover:underline"><ExternalLink size={13} />{t("setup.openDriver")}</button>
                    ) : c.installed ? (
                      <span className="text-[12px] text-[var(--color-accent)]">{t("setup.installed")}</span>
                    ) : c.delivery === "bundled" ? (
                      <span className="text-[11px] text-[var(--color-muted)] max-w-[120px] inline-block leading-tight">{deliveryNote(c)}</span>
                    ) : (
                      <div className="flex flex-col items-end gap-1">
                        <span className="mono text-[11px] text-[var(--color-muted)]">{fmtBytes(c.size)}</span>
                        <button onClick={() => download([c.id])} disabled={busy}
                          className="inline-flex items-center gap-1 px-2 py-1 rounded-md bg-[var(--color-surface-2)] border border-[var(--color-border)] text-[11px] hover:border-[var(--color-accent)] disabled:opacity-40">
                          <Download size={12} />{t("setup.downloadOne")}</button>
                      </div>
                    )}
                  </div>
                </div>
              </div>
            );
          })}
        </div>

        <div className="mt-6 flex items-center gap-3">
          {status && !status.ready && (
            <button onClick={() => { sendTelemetryChoice(); download(selectedComps.filter((c) => c.delivery === "download" && !c.installed).map((c) => c.id)); }} disabled={busy || sel.size === 0 || noSpace} title={noSpace ? t("downloads.noSpace", { need: fmtBytes(selectedSpace), free: fmtBytes(status.freeBytes ?? 0) }) : undefined}
              className="inline-flex items-center gap-2 px-4 py-2.5 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-sm font-semibold disabled:opacity-40 hover:brightness-105">
              {busy ? <Loader2 size={16} className="animate-spin" /> : <Download size={16} />}
              {t("setup.download")} {selectedBytes > 0 && <span className="opacity-80">· {fmtBytes(selectedBytes)}</span>}
            </button>
          )}
          {!busy && (
            <button onClick={async () => {
                try {
                  const r = await api.setupBrowse();
                  if (r.picked) { adopt(r.status); setPicked(null); }
                  if (r.errors.length > 0) setErr(`${t("downloads.importErrors")} · ${r.errors.join("; ")}`);
                } catch (e) { setErr(e instanceof SetupError ? `${errText(e.code)} · ${e.detail}` : String(e)); }
              }}
              className="inline-flex items-center gap-1.5 px-3 py-2.5 rounded-lg border border-dashed border-[var(--color-border)] text-sm text-[var(--color-muted)] hover:border-[var(--color-accent)] hover:text-[var(--color-text)] transition-colors">
              <FolderDown size={14} />{t("settings.browseFolder")}</button>
          )}
          {status?.ready && !busy && (
            <button onClick={() => { sendTelemetryChoice(); setStage("empty"); }}
              className="inline-flex items-center gap-2 px-4 py-2.5 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-sm font-semibold hover:brightness-105">
              <Check size={16} />{t("setup.continue")}</button>
          )}
        </div>
        <div className="mt-4 text-[13px] text-[var(--color-muted)]">
          <label className="inline-flex items-center gap-2 cursor-pointer">
            <input type="checkbox" className="h-4 w-4 accent-[var(--color-accent)]" checked={telemetryOn} onChange={(e) => { setTelemetryOn(e.target.checked); setTelemetry(e.target.checked, true).catch((err: Error) => console.warn("[hub] the statistics choice was not saved:", err.message)); }} />
            {t("hub.checkbox")}
          </label>
          <button type="button" onClick={() => setTelemetryPreview(true)} className="ml-2 underline underline-offset-2 hover:text-[var(--color-text)]">{t("hub.what")}</button>
          {telemetryPreview && <HubTelemetryPreview onClose={() => setTelemetryPreview(false)} />}
        </div>
      </motion.div>
    </div>
  );
}

// Пакетная обработка: DropZone кладёт выбранные файлы + настройки сюда, BatchView читает (без раздувания стора).
const batchState: { files: File[]; tgt: string; src: string; audio: string; subs: string; burn: boolean; detectText: boolean; funnyOn: boolean; funny: string; voGain: number; trStyle: string; keepOrig: boolean; container: "mp4" | "mkv"; voiceSrc: "clone" | "library"; slotsM: string[]; slotsF: string[] } =
  { files: [], tgt: "ru", src: "auto", audio: "dub", subs: "translate", burn: true, detectText: false, funnyOn: false, funny: "", voGain: -12, trStyle: "", keepOrig: false, container: "mp4", voiceSrc: "clone", slotsM: [], slotsF: [] };

type BatchItem = { name: string; status: "queued" | "analyzing" | "rendering" | "done" | "error"; pid: string | null; pct: number; msg?: string; detail?: string };

// Пакетный режим: пачка файлов -> для каждого createProject -> analyze -> (dub/funny) render, последовательно
// (движок одно-воркерный, GPU сериализует). Переиспользует существующие эндпоинты, отдельного backend не нужно.
function BatchView() {
  const { t } = useTranslation();
  const setStage = useStore((s) => s.setStage);
  const filesRef = useRef<File[]>(batchState.files);
  const { tgt, src, audio, subs, burn, detectText, funnyOn, funny, voGain, trStyle, keepOrig, container, voiceSrc, slotsM, slotsF } = batchState;
  const [items, setItems] = useState<BatchItem[]>(() => filesRef.current.map((f) => ({ name: f.name, status: "queued", pid: null, pct: 0 })));
  const [running, setRunning] = useState(false);
  const [doneN, setDoneN] = useState(0);
  const [saving, setSaving] = useState(false);

  const eMode = audio;
  const eSubs = audio === "transcribe" ? "transcribe" : subs;
  const eRewrite = funnyOn && (audio === "dub" || audio === "voiceover") ? funny.trim() : "";
  const doRender = audio === "dub" || audio === "voiceover";

  async function start() {
    setRunning(true);
    for (let i = 0; i < filesRef.current.length; i++) {
      const upd = (patch: Partial<BatchItem>) => setItems((xs) => xs.map((x, j) => (j === i ? { ...x, ...patch } : x)));
      try {
        upd({ status: "analyzing", pct: 0 });
        const bf = filesRef.current[i];
        const ao = isAudioFile(bf);                                // этот файл — аудио (без видео)?
        const { project_id } = await api.createProject(bf);
        upd({ pid: project_id });
        // Транскрипт всегда вжигает субтитры (иначе транскрипт-режим дал бы видео без текста); аудио-файл ->
        // субтитры/бёрн/OCR off (нет видео) -> на выходе озвученный WAV.
        const fSubs = ao ? "none" : eSubs;
        const fBurn = ao ? false : audio === "transcribe" ? true : burn;
        // Стиль перевода (#112) — параметром analyze (patch до analyze невозможен: project.json ещё нет).
        // Громкость оригинала, 2-я дорожка (#113) и голоса из библиотеки (#114) сервер кладёт на проект в
        // конце анализа (и повторяет при «Продолжить»); ненайденные голоса — строкой в журнале, спикеры на клоне.
        const post: AnalyzePost = {
          voGain: audio === "voiceover" ? voGain : undefined,
          keepOriginal: keepOrig && !ao && doRender ? { container } : undefined,
          voiceSlots: voiceSrc === "library" && doRender && (slotsM.length || slotsF.length) ? { male: slotsM, female: slotsF } : undefined,
        };
        const { job_id } = await api.analyze(project_id, tgt, eMode, src, fSubs, eRewrite, fBurn, ao ? false : detectText, false, trStyle, false, "", "auto", false, post);
        const res = await api.watchJob(job_id, (e) => { if (e.type === "progress") upd({ pct: e.pct ?? 0, detail: e.msg || undefined }); });
        reportVoiceSlots((res as AnalyzeResult).post.voice_slots);
        if (doRender) {
          upd({ status: "rendering", pct: 0 });
          const r = await api.render(project_id);
          await api.watchJob(r.job_id, (e) => { if (e.type === "progress") upd({ pct: e.pct ?? 0, detail: e.msg || undefined }); });
        }
        upd({ status: "done", pct: 100 });
      } catch (err) {
        upd({ status: "error", msg: String(err) });
      }
      setDoneN((n) => n + 1);
    }
    setRunning(false);
  }

  // Сохранить все готовые выходы в ОДНУ выбранную папку под ИСХОДНЫМИ именами (запрос юзера).
  async function saveAll() {
    const { dir } = await api.pickFolder();
    if (!dir) return;
    setSaving(true);
    for (const it of items) {
      if (it.status === "done" && it.pid) {
        try { await api.saveOutput(it.pid, dir, it.name); } catch { /* пропускаем отдельный файл */ }
      }
    }
    setSaving(false);
  }

  const icon = (s: BatchItem["status"]) =>
    s === "done" ? <Check size={15} className="text-[var(--color-accent)]" /> :
    s === "error" ? <X size={15} className="text-[var(--color-warn)]" /> :
    s === "queued" ? <Square size={13} className="text-[var(--color-muted)]" /> :
    <Loader2 size={15} className="animate-spin text-[var(--color-accent)]" />;

  return (
    <main className="flex-1 min-h-0 overflow-hidden flex flex-col p-4">
      <div className="max-w-3xl w-full mx-auto flex flex-col min-h-0">
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <button onClick={() => setStage("empty")} className="text-[13px] text-[var(--color-muted)] hover:text-[var(--color-text)] inline-flex items-center gap-1"><ArrowRight size={14} className="rotate-180" />{t("batch.back")}</button>
            <span className="text-[15px] font-semibold flex items-center gap-2"><FolderDown size={16} className="text-[var(--color-accent)]" />{t("batch.title")}</span>
          </div>
          <span className="text-[12px] text-[var(--color-muted)]">{doneN}/{items.length} · {t("batch.filesCount", { count: items.length })}</span>
        </div>

        <div className="flex-1 min-h-0 overflow-y-auto rounded-xl border border-[var(--color-border)] bg-[var(--color-surface)] divide-y divide-[var(--color-border)]">
          {items.map((it, i) => (
            <div key={i} className="flex items-center gap-3 px-3 py-2.5">
              <span className="shrink-0">{icon(it.status)}</span>
              <div className="min-w-0 flex-1">
                <div className="text-[13px] truncate">{it.name}</div>
                {(it.status === "analyzing" || it.status === "rendering") && (
                  <div className="mt-1 h-1 rounded bg-[var(--color-surface-2)] overflow-hidden">
                    <div className="h-full bg-[var(--color-accent)] transition-all" style={{ width: `${Math.min(100, Math.round(it.pct || 0))}%` }} />
                  </div>
                )}
                {it.status === "error" && <div className="text-[10px] text-[var(--color-warn)] truncate mono">{it.msg}</div>}
              </div>
              <span className="text-[11px] text-[var(--color-muted)] shrink-0">{t(`batch.${it.status}`)}</span>
              {it.status === "done" && it.pid && (
                <button onClick={() => it.pid && api.openOutput(it.pid).catch(() => {})} className="text-[11px] text-[var(--color-accent)] inline-flex items-center gap-1 shrink-0 hover:underline"><ExternalLink size={12} />{t("batch.open_out")}</button>
              )}
            </div>
          ))}
        </div>

        <button onClick={start} disabled={running || items.length === 0}
          className="mt-3 w-full inline-flex items-center justify-center gap-2 px-4 py-2.5 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-sm font-semibold disabled:opacity-50 hover:brightness-105 transition">
          {running ? <Loader2 size={15} className="animate-spin" /> : <Play size={15} />}{t("batch.start")}
        </button>
        {doneN > 0 && (
          <button onClick={saveAll} disabled={saving || running}
            className="mt-2 w-full inline-flex items-center justify-center gap-2 px-4 py-2 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] text-sm font-medium disabled:opacity-50 hover:bg-[var(--color-surface-2)] transition">
            {saving ? <Loader2 size={15} className="animate-spin" /> : <FolderDown size={15} />}{t("batch.saveAll")}
          </button>
        )}
      </div>
    </main>
  );
}

// Мультиязык. Два режима:
//  • создание (sourcePid=null): DropZone кладёт ОДИН файл + языки -> createProject/analyze/render с нуля.
//  • экспорт (sourcePid задан): редактор кладёт pid готового проекта -> exportLang (клон+ре-перевод+рендер),
//    наследуя ВСЕ правки (раскладку/стиль/блюр/титры/голос). file тогда null.
const multiLangState: { sourcePid: string | null; sourceName: string; langs: string[] } =
  { sourcePid: null, sourceName: "", langs: [] };

type MLItem = { lang: string; status: "queued" | "analyzing" | "rendering" | "done" | "error"; pid: string | null; pct: number; msg?: string; detail?: string };

// Мультиязычный перевод: ОДИН ролик -> сразу на N языков. Для каждого языка createProject(тот же файл) ->
// analyze(tgt=язык) -> render -> output. Последовательно (движок одно-воркерный, GPU сериализует).
// Переиспользует те же эндпоинты, что batch/DropZone — отдельного backend не нужно. Результат: N проектов.
function MultiLangView() {
  const { t } = useTranslation();
  const setStage = useStore((s) => s.setStage);
  const setPid = useStore((s) => s.setPid);
  const setProject = useStore((s) => s.setProject);
  const setRendered = useStore((s) => s.setRendered);
  const srcPid = multiLangState.sourcePid;                              // pid готового проекта -> экспорт на N языков
  const { langs, sourceName } = multiLangState;
  const [items, setItems] = useState<MLItem[]>(() => langs.map((l) => ({ lang: l, status: "queued", pid: null, pct: 0 })));
  const [running, setRunning] = useState(false);
  const [doneN, setDoneN] = useState(0);
  const langName = (code: string) => DUB_LANGS.find((l) => l.code === code)?.name ?? code.toUpperCase();
  const mounted = useRef(true);                                         // не дёргаем локальный setState после размонтирования
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);  // setup обязателен: StrictMode remount иначе оставит false навсегда

  async function run() {
    if (!srcPid || running) return;
    setRunning(true);
    for (let i = 0; i < langs.length; i++) {
      const upd = (patch: Partial<MLItem>) => { if (mounted.current) setItems((xs) => xs.map((x, j) => (j === i ? { ...x, ...patch } : x))); };
      try {
        // Клон готового проекта + ре-перевод + рендер ОДНИМ джобом (наследует все правки).
        upd({ status: "rendering", pct: 0 });
        useStore.getState().pushActivity(`${langName(langs[i])} — ${t("multilang.title")}`, "work");
        const { job_id, project_id } = await api.exportLang(srcPid, langs[i]);
        upd({ pid: project_id });
        // Глобальный прогресс: чтобы статус/журнал показывали работу даже если ушли в редактор.
        await api.watchJob(job_id, (e) => { if (e.type === "progress") { upd({ pct: e.pct ?? 0, detail: e.msg || undefined }); useStore.getState().setProgress("multilang", `${langName(langs[i])}: ${e.msg || ""}`, e.pct ?? null); } });
        upd({ status: "done", pct: 100 });
      } catch (err) {
        upd({ status: "error", msg: String(err) });
      }
      if (mounted.current) setDoneN((n) => n + 1);
    }
    if (mounted.current) setRunning(false);
    useStore.getState().setProgress("", "", null);   // очистить глобальный статус по завершении всех языков
  }
  // Стартуем один раз: гвард от двойного mount в React 19 StrictMode (иначе экспорт запустится ДВАЖДЫ).
  const started = useRef(false);
  useEffect(() => { if (started.current) return; started.current = true; run(); }, []);   // eslint-disable-line react-hooks/exhaustive-deps

  async function openInEditor(pid: string) {
    try {
      const p = await api.getProject(pid);
      setPid(pid); setProject(p); setRendered(false);   // сброс: покадровое превью, а не старое output-видео (чёрный кадр)
      setStage("editor");
      window.history.pushState(null, "", `?pid=${pid}`);
    } catch { /* пропало */ }
  }
  const icon = (s: MLItem["status"]) =>
    s === "done" ? <Check size={15} className="text-[var(--color-accent)]" /> :
    s === "error" ? <X size={15} className="text-[var(--color-warn)]" /> :
    s === "queued" ? <Square size={13} className="text-[var(--color-muted)]" /> :
    <Loader2 size={15} className="animate-spin text-[var(--color-accent)]" />;

  return (
    <main className="flex-1 min-h-0 overflow-hidden flex flex-col p-4">
      <div className="max-w-3xl w-full mx-auto flex flex-col min-h-0">
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <button onClick={() => setStage(srcPid ? "editor" : "empty")} className="text-[13px] text-[var(--color-muted)] hover:text-[var(--color-text)] inline-flex items-center gap-1"><ArrowRight size={14} className="rotate-180" />{t("batch.back")}</button>
            <span className="text-[15px] font-semibold flex items-center gap-2"><Languages size={16} className="text-[var(--color-accent)]" />{t("multilang.title")}</span>
          </div>
          <span className="text-[12px] text-[var(--color-muted)]">{doneN}/{items.length} · {t("multilang.langsCount", { count: items.length })}</span>
        </div>
        <div className="text-[12px] text-[var(--color-muted)] mb-2 truncate">{sourceName} → {t("multilang.langsCount", { count: items.length })}</div>

        <div className="flex-1 min-h-0 overflow-y-auto rounded-xl border border-[var(--color-border)] bg-[var(--color-surface)] divide-y divide-[var(--color-border)]">
          {items.map((it, i) => (
            <div key={i} className="flex items-center gap-3 px-3 py-2.5">
              <span className="shrink-0">{icon(it.status)}</span>
              <div className="min-w-0 flex-1">
                <div className="text-[13px] truncate">{langName(it.lang)}</div>
                {(it.status === "analyzing" || it.status === "rendering") && (
                  <div className="mt-1 h-1 rounded bg-[var(--color-surface-2)] overflow-hidden">
                    <div className="h-full bg-[var(--color-accent)] transition-all" style={{ width: `${Math.min(100, Math.round(it.pct || 0))}%` }} />
                  </div>
                )}
                {it.status === "error" && <div className="text-[10px] text-[var(--color-warn)] truncate mono">{it.msg}</div>}
              </div>
              <span className="text-[11px] text-[var(--color-muted)] shrink-0">{t(`batch.${it.status}`)}</span>
              {it.status === "done" && it.pid && (
                <>
                  <button onClick={() => it.pid && openInEditor(it.pid)} className="text-[11px] text-[var(--color-muted)] inline-flex items-center gap-1 shrink-0 hover:text-[var(--color-text)]"><Eye size={12} />{t("multilang.openEditor")}</button>
                  <button onClick={() => { if (it.pid) api.openOutput(it.pid).catch(() => { if (it.pid) openInEditor(it.pid); }); }} className="text-[11px] text-[var(--color-accent)] inline-flex items-center gap-1 shrink-0 hover:underline"><ExternalLink size={12} />{t("batch.open_out")}</button>
                </>
              )}
            </div>
          ))}
        </div>
        {!running && doneN === items.length && (
          <button onClick={() => setStage("empty")}
            className="mt-3 w-full inline-flex items-center justify-center gap-2 px-4 py-2.5 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-sm font-semibold hover:brightness-105 transition">
            <Check size={15} />{t("multilang.done")}
          </button>
        )}
      </div>
    </main>
  );
}

// Режим «Транскрипт»: диаризованный транскрипт (analyze mode=transcribe) + создание голосов из спикеров
// (speaker-voice, ref-текст авто-транскрибируется на рендере) + экспорт .srt/.txt. Отдельный экран, не Editor.
// Цвет на каждого из 8 спикеров, которых различает диаризация (Nemotron 3 Diarization).
const SPK_PALETTE = ["#7fb3ff", "#f79bd3", "#c6f24e", "#ffb454", "#b79cff", "#5fe0c8", "#ff8a7a", "#f2e15c"];

function TranscriptView() {
  const { t, i18n } = useTranslation();
  const p = useStore((s) => s.project) as Project;
  const pid = useStore((s) => s.pid) as string;
  const setStage = useStore((s) => s.setStage);                     // выход на главный экран (кнопка «Назад»)
  const setProject = useStore((s) => s.setProject);
  const setProgress = useStore((s) => s.setProgress);
  const setJobSteps = useStore((s) => s.setJobSteps);               // #122/#3: шаги степпера нового прогона
  const setAudioOnly = useStore((s) => s.setAudioOnly);            // «analyzing» знает видео/аудио вход
  const [, setPortalTick] = useState(0);                            // 2-й рендер: слот #editor-modes-slot уже в DOM (как forcePortal в Editor)
  useEffect(() => { setPortalTick(1); }, []);
  // Переключатель режимов из транскрипта (реквест Стаса #122): симметрично с Editor. transcribe НЕ переводит
  // (tgt=исходный текст) -> переход в дубляж/субтитры/закадр = только ПЕРЕВОД уже готовых сегментов
  // (retranslate, БЕЗ повторной сепарации/диаризации/ASR — текст только что распознан, незачем гонять заново).
  // После перевода projMode сменится -> отрисуется Editor; озвучка/сборка идут на рендере, как обычно.
  const TR_MODES = [["subtitles", Captions], ["dub", AudioLines], ["voiceover", Mic2], ["funny", Sparkles], ["transcribe", FileText]] as const;
  // #122/#4: свой гейт ре-анализа. Старый `busy` — стейт СОЗДАНИЯ ГОЛОСОВ, switchMode его не ставил ->
  // двойной клик по разным режимам запускал 2 analyze. reanalyzing блокирует и дизейблит кнопки портала.
  const [reanalyzing, setReanalyzing] = useState(false);
  const trAudioOnly = !((p.meta.width || 0) > 0 && (p.meta.height || 0) > 0);   // транскрипт может быть аудио-входом
  async function switchMode(k: string) {
    if (k === "transcribe" || reanalyzing) return;                  // уже в транскрипте / ре-анализ уже идёт
    setReanalyzing(true);
    const mode = k === "subtitles" ? "nodub" : k === "funny" ? "dub" : k;   // dub|voiceover|nodub
    const tgt = (i18n.language as string) || p.tgt_lang || "ru";
    setJobSteps(["translating"]);
    setAudioOnly(trAudioOnly);
    setProgress("", "", null);
    setStage("analyzing");
    try {
      const { job_id } = await api.retranslate(pid, tgt, mode);
      await watchTracked(pid, "retranslate", job_id);
      const updated = await api.getProject(pid);
      if (updated.mode === "transcribe") {
        const patched = await api.patch(pid, { mode });
        setProject(patched);
      } else {
        setProject(updated);
      }
      setStage("editor");
    } catch (err) {
      if (err instanceof JobCancelledError) {                        // перевод отменили на экране анализа: транскрипт остаётся транскриптом
        useStore.getState().pushActivity(t("jobs.cancelledKind", { kind: t("jobs.kind.retranslate") }), "done");
        try { setProject(await api.getProject(pid)); }
        catch (e) { useStore.getState().pushActivity(String(e), "error"); }
        setStage("editor");
        return;
      }
      useStore.getState().pushActivity(err instanceof Error ? err.message : String(err), "error");
      try { setProject(await api.getProject(pid)); }
      catch (e) { useStore.getState().pushActivity(String(e), "error"); }
      setStage("editor");
    } finally { setReanalyzing(false); }
  }
  const [busy, setBusy] = useState<string | null>(null);            // спикер в работе, либо "__all__"
  const [made, setMade] = useState<Record<string, string>>({});     // спикер -> имя созданного голоса
  const [scrub, setScrub] = useState(() => initialScrub() || 0);      // ?t=SEC — deep-link на кадр транскрипта
  const [play, setPlay] = useState(false);
  const [showHelp, setShowHelp] = useState(false);                  // оверлей-шпаргалка хоткеев (?)
  const videoRef = useRef<HTMLVideoElement>(null);
  const previewRef = useRef<HTMLDivElement>(null);                  // контейнер <video> -> фулскрин по F + Ctrl-колесо
  const scrubRef = useRef(0);                                       // свежий scrub без stale-замыкания в хоткеях
  // Плей: <video> тянет /dub — для транскрипта (nodub) это ОРИГИНАЛ (видео+аудио). Скраб следует за
  // currentTime -> активная строка + караоке-подсветка слова. Один элемент = и картинка, и звук.
  useEffect(() => {
    const v = videoRef.current; if (!v) return;
    if (!play) { v.pause(); return; }
    v.play().catch(() => setPlay(false));
    const id = window.setInterval(() => setScrub(v.currentTime), 80);
    return () => window.clearInterval(id);
  }, [play]);
  const seek = (tt: number) => { setScrub(tt); if (videoRef.current) videoRef.current.currentTime = tt; };
  useEffect(() => { scrubRef.current = scrub; }, [scrub]);          // свежий scrub для хоткеев
  useMediaHotkeys({                                                 // громкости нет -> ↑/↓ пропущены (setVol не задан)
    enabled: true, duration: p.meta.duration || 0, scrubRef, seek,
    togglePlay: () => setPlay((x) => !x), previewRef, setHelp: setShowHelp,
    blocked: () => document.querySelector(".glass-scrim") != null,
  });
  useTranscriptBridge({ pid, scrub, playing: play, seek, play: () => setPlay(true), pause: () => setPlay(false), switchMode: (k) => void switchMode(k), switching: reanalyzing });
  useProjectSync(pid, () => null);
  // пословные тайминги ASR (лежат в extra.words) — для караоке внутри активной фразы
  const wordsOf = (s: Project["segments"][number]) =>
    (((s as unknown as { extra?: { words?: Array<{ word: string; start: number; end: number }> } }).extra?.words) || []);

  const rows = p.segments.filter((s) => (s.src_text || "").trim());
  const speakers = [...new Set(p.segments.map((s) => s.speaker ?? "0"))].sort();
  const colorOf = (spk: string) => SPK_PALETTE[Math.max(0, speakers.indexOf(spk)) % SPK_PALETTE.length];
  // активная фраза = та, чей [start,end] накрывает скраб; при смене — подсветка + автоскролл к ней
  const activeId = rows.find((s) => scrub >= s.start && scrub < (s.end > s.start ? s.end : s.start + 3))?.id;
  const activeRef = useRef<HTMLDivElement>(null);
  useEffect(() => { activeRef.current?.scrollIntoView({ block: "nearest", behavior: "smooth" }); }, [activeId]);
  const durOf = (spk: string) => p.segments.filter((s) => (s.speaker ?? "0") === spk).reduce((a, s) => a + Math.max(0, s.end - s.start), 0);
  const fmt = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;

  async function makeVoice(spk: string) {
    setBusy(spk);
    try { const r = await makeSpeakerVoice(pid, spk, `${t("transcribe.speaker")} ${spk}`); if (r) setMade((m) => ({ ...m, [spk]: r.name })); }
    finally { setBusy(null); }
  }
  async function makeAll() {
    setBusy("__all__");
    try {
      for (const spk of speakers) {
        const r = await makeSpeakerVoice(pid, spk, `${t("transcribe.speaker")} ${spk}`);
        if (!r) break;
        setMade((m) => ({ ...m, [spk]: r.name }));
      }
    } finally { setBusy(null); }
  }

  async function dl(name: string, text: string) {
    // В нативном Tauri-webview браузерный blob-download молча не срабатывает (баг «Экспорт SRT не работает»).
    // Пишем файл на бэке в каталог проекта и открываем проводник с выделением. Фолбэк — blob (dev-браузер).
    try {
      await api.saveText(pid, name, text);
    } catch {
      const blob = new Blob([text], { type: "text/plain;charset=utf-8" });
      const a = document.createElement("a"); a.href = URL.createObjectURL(blob); a.download = name; a.click();
      setTimeout(() => URL.revokeObjectURL(a.href), 1000);
    }
  }
  const srtTime = (s: number) => {
    const ms = Math.max(0, Math.round(s * 1000)), z = (n: number, w = 2) => String(n).padStart(w, "0");
    return `${z(Math.floor(ms / 3600000))}:${z(Math.floor((ms % 3600000) / 60000))}:${z(Math.floor((ms % 60000) / 1000))},${z(ms % 1000, 3)}`;
  };
  const exportSrt = () => dl("transcript.srt", rows.map((s, i) => `${i + 1}\n${srtTime(s.start)} --> ${srtTime(s.end)}\n${(s.src_text || "").trim()}\n`).join("\n"));
  const exportTxt = () => dl("transcript.txt", rows.map((s) => `[${t("transcribe.speaker")} ${s.speaker ?? "0"}] ${(s.src_text || "").trim()}`).join("\n"));

  return (
    <main className="flex-1 min-h-0 overflow-hidden grid grid-cols-[1.4fr_1fr] gap-3 p-3">
      {(() => { const el = document.getElementById("editor-modes-slot"); return el ? createPortal(
        <div className="inline-flex rounded-lg bg-[var(--color-surface-2)] p-0.5 border border-[var(--color-border)] shrink-0">
          {TR_MODES.map(([k, Ic]) => (
            <button key={k} onClick={() => switchMode(k)} title={t(`mode.${k}_desc`)} disabled={reanalyzing} data-mode={k} aria-pressed={k === "transcribe"}
              className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-[13px] transition-colors disabled:opacity-50 disabled:cursor-default ${k === "transcribe" ? "bg-[var(--color-accent)] text-[var(--color-on-accent)] font-semibold" : "text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
              <Ic size={15} /> <span className="hidden xl:inline">{t(`mode.${k}`)}</span>
            </button>
          ))}
        </div>, el) : null; })()}
      <div className="min-h-0 flex flex-col rounded-xl border border-[var(--color-border)] bg-[var(--color-surface)]">
        <div className="px-3 py-2 flex items-center justify-between border-b border-[var(--color-border)]">
          <span className="text-[13px] font-medium flex items-center gap-2 min-w-0">
            <button onClick={() => setStage("empty")} title={t("batch.back")} className="shrink-0 text-[var(--color-muted)] hover:text-[var(--color-text)] inline-flex items-center gap-1"><ArrowRight size={14} className="rotate-180" /><span className="hidden sm:inline text-[12px]">{t("batch.back")}</span></button>
            <FileText size={15} className="text-[var(--color-accent)] shrink-0" /><span className="truncate">{p.meta.video ? baseName(p.meta.video) : t("transcribe.title")}</span>
          </span>
          <span className="text-[11px] text-[var(--color-muted)] shrink-0">{t("transcribe.speakersCount", { count: speakers.length })}</span>
        </div>
        <div className="px-3 pt-2 space-y-2">
          <div ref={previewRef} className="fs-preview relative rounded-lg overflow-hidden bg-black/50 border border-[var(--color-border)] grid place-items-center max-h-[34vh]">
            <video ref={videoRef} src={api.dubUrl(pid)} playsInline preload="auto"
              onEnded={() => setPlay(false)} onClick={() => setPlay((x) => !x)}
              className="max-h-[34vh] max-w-full cursor-pointer" />
            <button onClick={() => setPlay((x) => !x)} title={play ? t("common.pause") : t("common.play")}
              className="absolute bottom-2 left-2 grid place-items-center w-10 h-10 rounded-full bg-[var(--color-accent)] text-[var(--color-on-accent)] shadow-lg hover:brightness-110 transition">
              {play ? <Pause size={18} /> : <Play size={18} className="ml-0.5" />}
            </button>
          </div>
          <WaveformTimeline pid={pid} duration={p.meta.duration || 0} scrub={scrub} segments={p.segments} onSeek={seek} />
        </div>
        <div data-kb-scroll className="flex-1 min-h-0 overflow-y-auto px-3 py-2 space-y-1.5">
          {rows.map((s) => {
            const spk = s.speaker ?? "0";
            const active = s.id === activeId;
            const words = active ? wordsOf(s) : [];
            return (
              <div key={s.id} ref={active ? activeRef : undefined} onClick={() => seek(s.start)} data-seg-id={s.id}
                data-mcp-context={`segment ${s.id} ${fmt(s.start)} SPK ${spk}: ${(s.src_text || "").trim().slice(0, 60)}`}
                className={`flex gap-2 items-start cursor-pointer rounded px-1.5 -mx-1.5 py-0.5 transition-colors ${active ? "bg-[color-mix(in_oklab,var(--color-accent)_16%,transparent)]" : "hover:bg-[var(--color-surface-2)]"}`}>
                <span className="mono text-[9px] px-1.5 py-px rounded shrink-0" style={{ background: colorOf(spk), color: "#0b0c0e" }}>SPK {spk}</span>
                <span className="mono text-[9px] text-[var(--color-muted)] pt-0.5 shrink-0 w-8">{fmt(s.start)}</span>
                <FitBadge fit={s.fit} />
                <span className="text-[13px] leading-snug">
                  {active && words.length > 0
                    ? words.map((w, i) => (
                        <span key={i} className={`transition-colors ${scrub >= w.start && scrub < w.end ? "bg-[var(--color-accent)] text-[var(--color-on-accent)] rounded px-0.5" : ""}`}>{w.word}{" "}</span>
                      ))
                    : (s.src_text || "").trim()}
                </span>
              </div>
            );
          })}
          {rows.length === 0 && <div className="text-[12px] text-[var(--color-muted)] py-6 text-center">{t("transcribe.empty")}</div>}
        </div>
      </div>

      <div className="min-h-0 flex flex-col gap-3">
        <div className="rounded-xl border border-[var(--color-border)] bg-[var(--color-surface)] p-3">
          <div className="text-[12px] text-[var(--color-muted)] mb-2">{t("transcribe.speakers")}</div>
          <div>
            {speakers.map((spk) => (
              <div key={spk} className="flex items-center justify-between py-1.5 border-b border-[var(--color-border)] last:border-0">
                <div className="flex items-center gap-2">
                  <span className="w-2.5 h-2.5 rounded-full shrink-0" style={{ background: colorOf(spk) }} />
                  <div>
                    <div className="text-[13px]">{t("transcribe.speaker")} {spk}</div>
                    <div className="text-[10px] text-[var(--color-muted)]">{durOf(spk).toFixed(1)}{t("transcribe.sec")}{made[spk] ? ` · ${made[spk]}` : ""}</div>
                  </div>
                </div>
                <button onClick={() => makeVoice(spk)} disabled={busy !== null}
                  className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg border border-[#37414d] text-[11px] text-[var(--color-accent)] hover:border-[var(--color-accent)] disabled:opacity-40">
                  {busy === spk ? <Loader2 size={12} className="animate-spin" /> : made[spk] ? <Check size={12} /> : <Mic2 size={12} />}{t("transcribe.makeVoice")}
                </button>
              </div>
            ))}
          </div>
          <div className="text-[10px] text-[var(--color-muted)] mt-2 flex items-center gap-1.5"><Sparkles size={11} className="text-[var(--color-accent)] shrink-0" />{t("transcribe.refHint")}</div>
        </div>

        <button onClick={makeAll} disabled={busy !== null}
          className="w-full inline-flex items-center justify-center gap-2 px-3 py-2 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-sm font-semibold disabled:opacity-50 hover:brightness-105 transition">
          {busy === "__all__" ? <Loader2 size={15} className="animate-spin" /> : <Users size={15} />}{t("transcribe.makeAll")}
        </button>
        <div className="flex gap-2">
          <GlossaryButton pid={pid} project={p} wide />
          <button onClick={exportSrt} className="flex-1 inline-flex items-center justify-center gap-1.5 px-3 py-2 rounded-lg border border-[#37414d] text-[12px] hover:border-[var(--color-accent)]"><Download size={13} />{t("transcribe.exportSrt")}</button>
          <button onClick={exportTxt} className="flex-1 inline-flex items-center justify-center gap-1.5 px-3 py-2 rounded-lg border border-[#37414d] text-[12px] hover:border-[var(--color-accent)]"><FileText size={13} />{t("transcribe.exportTxt")}</button>
        </div>
      </div>
      {showHelp && <ShortcutsHelp onClose={() => setShowHelp(false)} />}
    </main>
  );
}

export default function App() {
  const { t, i18n } = useTranslation();
  const stage = useStore((s) => s.stage);                 // only re-route on stage change (not on every store write)
  // Анонимная статистика включена по умолчанию: на первом запуске решает галочка «Первого запуска», а установка,
  // обновившаяся мимо него, остаётся при умолчании, пока галочку не снимут в Настройках.
  const hub = useHubState(i18n.language);
  useEffect(() => {
    const tel = hub?.telemetry;
    if (stage === "setup" || !tel || tel.acknowledged || tel.disabledByEnv) return;
    setTelemetry(tel.enabled, true).catch((e: Error) => console.warn("[hub] the statistics default was not saved:", e.message)).finally(hubChanged);
  }, [stage, hub?.telemetry.acknowledged]);
  const projMode = useStore((s) => (s.project as Project | null)?.mode);   // transcribe -> отдельный экран
  const setPid = useStore((s) => s.setPid);
  const setProject = useStore((s) => s.setProject);
  const setStage = useStore((s) => s.setStage);
  const pid = useStore((s) => s.pid);
  const [cap, setCap] = useState("");
  const [capOffline, setCapOffline] = useState(false);
  useEffect(() => { api.capabilities().then((c) => {
    const base = (p?: string) => p ? (p.split(/[\\/]/).pop() || p).replace(/\.(gguf|onnx|bin|pt|safetensors)$/i, "") : "";
    // ASR в футере — из ФАКТИЧЕСКОГО выбора (active.json), не из статичного имени Parakeet-модели:
    // юзер переключил движок в «Моделях» -> строка обязана показать то, чем реально пойдёт прогон.
    const sel = (c as { selection?: Record<string, string> }).selection ?? {};
    const asrLabel = sel.asr_engine === "whisper" ? `whisper ${sel.whisper_model || "auto"}` : `${c.asr_model}${sel.asr ? ` ${sel.asr}` : ""}`;
    const parts = [c.device, `ASR ${asrLabel}`];
    if (c.models?.llm) parts.push(`MT+vision ${base(c.models.llm)}`);
    if (c.models?.tts) parts.push(`TTS ${c.models.tts}${c.tts_quant ? ` ${c.tts_quant}` : ""}`);
    parts.push("sep BSRoformer", "diar Nemotron 3", "OCR PP-OCR");   // фикс-движки пайплайна
    setCap(parts.join(" · "));
  }).catch(() => setCapOffline(true)); }, []);
  // boot: resume ?pid=… project, else gate on /setup/status — missing required components -> «first run».
  // Сетевая ошибка (fetch отклонён TypeError) = сервиса нет на порту -> экран «сервер не отвечает»; HTTP-ошибка
  // значит, что сервис ответил, и ведёт на главный экран.
  useEffect(() => {
    const unreachable = (e: unknown) => e instanceof TypeError;
    const pid = new URLSearchParams(location.search).get("pid");
    if (pid) { api.getProject(pid).then((p) => { setPid(pid); setProject(p); setStage("editor"); }).catch((e: unknown) => setStage(unreachable(e) ? "offline" : "empty")); return; }
    api.setupStatus()
      .then((s) => setStage(s.ready ? "empty" : "setup"))
      .catch((e: unknown) => setStage(unreachable(e) ? "offline" : "empty"));   // older server without /setup -> fall through to hero
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
  return (
    <div className="h-full flex flex-col">
      <TopBar />
      {stage === "boot" && <div className="flex-1 grid place-items-center"><Loader2 size={22} className="animate-spin text-[var(--color-muted)]" /></div>}
      {stage === "offline" && <ServerOffline />}
      {stage === "setup" && <FirstRun />}
      {stage === "empty" && <DropZone />}
      {stage === "analyzing" && <AnalyzeProgress />}
      {stage === "batch" && <BatchView />}
      {stage === "multilang" && <MultiLangView />}
      {stage === "editor" && (projMode === "transcribe" ? <TranscriptView /> : <Editor />)}
      {stage === "editor" && pid && <ProjectJobBar key={pid} pid={pid} />}
      <footer className="mono h-6 px-4 flex items-center gap-2 text-[10px] text-[var(--color-muted)] border-t border-[var(--color-border)] bg-[var(--color-surface)] overflow-hidden">
        <span className="w-1.5 h-1.5 rounded-full bg-[var(--color-accent)] shrink-0" /><span className="truncate" title={capOffline ? t("status.backendOffline") : cap}>{capOffline ? t("status.backendOffline") : cap}</span>
      </footer>
      <FilesPanel />
      <BridgeHost />
      {(stage === "editor" || stage === "analyzing" || stage === "multilang" || stage === "batch") && <ResourceMonitor />}
    </div>
  );
}
