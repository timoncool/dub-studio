import { create } from "zustand";
import type { Project } from "./lib/api";

type Stage = "empty" | "analyzing" | "editor";
export type ExportItem = { id: string; name: string; status: "rendering" | "done" | "error"; msg: string; url?: string };

type State = {
  stage: Stage;
  pid: string | null;
  project: Project | null;
  progress: { stage: string; msg: string };
  rendered: boolean;
  rendering: boolean;                // current project's export in flight (button state; does NOT block the screen)
  exports: ExportItem[];            // non-blocking export queue shown in the bottom-right Files panel
  past: Project[];                  // undo/redo history of Project snapshots
  future: Project[];
  rev: number;                       // preview cache-buster: bumped on every backend-confirmed frame change
  selBlur: number | null;            // selected blur-box index — SHARED between the left list and the canvas overlay
  setStage: (s: Stage) => void;
  setPid: (p: string | null) => void;
  setProject: (p: Project | null) => void;
  setProgress: (stage: string, msg: string) => void;
  setRendered: (b: boolean) => void;
  setRendering: (b: boolean) => void;
  addExport: (e: ExportItem) => void;
  updateExport: (id: string, patch: Partial<ExportItem>) => void;
  pushHistory: (p: Project) => void; // snapshot the project BEFORE a mutation (for undo)
  undo: () => Project | null;        // returns the project to restore (PUT it) or null
  redo: () => Project | null;
  bump: () => void;                  // invalidate the rendered preview frame -> <img> refetches
  setSelBlur: (i: number | null) => void;
};

export const useStore = create<State>((set, get) => ({
  stage: "empty",
  pid: null,
  project: null,
  progress: { stage: "", msg: "" },
  rendered: false,
  rendering: false,
  exports: [],
  past: [],
  future: [],
  rev: 0,
  selBlur: null,
  setStage: (stage) => set({ stage }),
  setPid: (pid) => set({ pid }),
  setProject: (project) => set({ project }),
  setProgress: (stage, msg) => set({ progress: { stage, msg } }),
  setRendered: (rendered) => set({ rendered }),
  setRendering: (rendering) => set({ rendering }),
  addExport: (e) => set((s) => ({ exports: [e, ...s.exports] })),
  updateExport: (id, patch) => set((s) => ({ exports: s.exports.map((x) => (x.id === id ? { ...x, ...patch } : x)) })),
  pushHistory: (p) => set((s) => {
    // no-op if the snapshot matches the current head: callers may re-snapshot the same
    // baseline (e.g. a second keystroke in the same edit burst), and an unconditional
    // future:[] there would silently kill the redo stack.
    const head = s.past[s.past.length - 1];
    if (head && JSON.stringify(head) === JSON.stringify(p)) return {};
    return { past: [...s.past, p].slice(-60), future: [] };
  }),
  undo: () => {
    const s = get(); if (!s.past.length || !s.project) return null;
    const prev = s.past[s.past.length - 1];
    set({ past: s.past.slice(0, -1), future: [s.project, ...s.future], project: prev });
    return prev;
  },
  redo: () => {
    const s = get(); if (!s.future.length || !s.project) return null;
    const next = s.future[0];
    set({ future: s.future.slice(1), past: [...s.past, s.project], project: next });
    return next;
  },
  bump: () => set((s) => ({ rev: s.rev + 1 })),
  setSelBlur: (selBlur) => set({ selBlur }),
}));
