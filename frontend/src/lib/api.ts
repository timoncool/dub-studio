// Dub Studio API client — talks to the single-worker FastAPI backend over the dub-engine.
const BASE = (import.meta.env.VITE_API as string) || "http://127.0.0.1:8765";

export type SubStyle = {
  color: string; outline: string; italic: boolean; bold: boolean; uppercase: boolean;
  font?: string | null; scene_color?: string | null; scene_flat: boolean;
  n_lines?: number | null; align: string; size_px?: number | null;
};
export type Segment = {
  id: string; start: number; end: number; speaker?: string | null;
  src_text: string; tgt_text: string; voice?: string | null; dirty: boolean;
};
export type BlurBox = { x: number; y: number; w: number; h: number; t0: number; t1: number };
export type Project = {
  meta: { video: string; duration: number; width: number; height: number; fps: number; src_codec: string };
  mode: string; tgt_lang: string;
  audio: { keep_music: boolean; voice: { mode: string; name?: string | null }; rewrite?: string | null };
  segments: Segment[];
  subs: { mode: string };
  captions: {
    sub_style?: SubStyle | null; sub_y?: number | null; overrides: unknown[];
    titles: unknown[]; brands: unknown[]; blur_boxes: BlurBox[]; preset: Record<string, unknown>;
  };
  render: { burn_cq: number; blur_sigma: number; codec: string };
  work_dir?: string | null;
};
export type Capabilities = {
  device: string; tts_quant: string; asr_model: string; ffmpeg: boolean;
  languages: string[]; voice_modes: string[];
};
export type JobEvent = { type: "progress" | "done" | "error"; stage?: string; pct?: number; msg?: string; result?: unknown; error?: string };

async function j<T>(r: Response): Promise<T> {
  if (!r.ok) throw new Error(`${r.status} ${await r.text()}`);
  return r.json() as Promise<T>;
}

export const api = {
  capabilities: () => fetch(`${BASE}/engine/capabilities`).then(j<Capabilities>),
  createProject: (file: File) => {
    const fd = new FormData(); fd.append("file", file);
    return fetch(`${BASE}/projects`, { method: "POST", body: fd }).then(j<{ project_id: string }>);
  },
  analyze: (pid: string, tgt_lang: string, mode = "auto") =>
    fetch(`${BASE}/projects/${pid}/analyze?tgt_lang=${tgt_lang}&mode=${mode}`, { method: "POST" }).then(j<{ job_id: string }>),
  getProject: (pid: string) => fetch(`${BASE}/projects/${pid}`).then(j<Project>),
  patch: (pid: string, edit: Record<string, unknown>) =>
    fetch(`${BASE}/projects/${pid}`, { method: "PATCH", headers: { "Content-Type": "application/json" }, body: JSON.stringify(edit) }).then(j<Project>),
  render: (pid: string) => fetch(`${BASE}/projects/${pid}/render`, { method: "POST" }).then(j<{ job_id: string }>),
  previewUrl: (pid: string, t: number, rev = 0) => `${BASE}/projects/${pid}/preview?t=${t}&rev=${rev}`,
  outputUrl: (pid: string) => `${BASE}/projects/${pid}/output`,
  // SSE job progress -> onEvent per message; resolves on done, rejects on error
  watchJob: (jobId: string, onEvent: (e: JobEvent) => void) =>
    new Promise<unknown>((resolve, reject) => {
      const es = new EventSource(`${BASE}/jobs/${jobId}/events`);
      es.onmessage = (m) => {
        const e: JobEvent = JSON.parse(m.data);
        onEvent(e);
        if (e.type === "done") { es.close(); resolve(e.result); }
        if (e.type === "error") { es.close(); reject(new Error(e.error)); }
      };
      es.onerror = () => { es.close(); reject(new Error("SSE connection lost")); };
    }),
};
