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
  rev: number;                       // preview cache-buster: bumped on every backend-confirmed frame change
  setStage: (s: Stage) => void;
  setPid: (p: string | null) => void;
  setProject: (p: Project | null) => void;
  setProgress: (stage: string, msg: string) => void;
  setRendered: (b: boolean) => void;
  setRendering: (b: boolean) => void;
  addExport: (e: ExportItem) => void;
  updateExport: (id: string, patch: Partial<ExportItem>) => void;
  bump: () => void;                  // invalidate the rendered preview frame -> <img> refetches
};

export const useStore = create<State>((set) => ({
  stage: "empty",
  pid: null,
  project: null,
  progress: { stage: "", msg: "" },
  rendered: false,
  rendering: false,
  exports: [],
  rev: 0,
  setStage: (stage) => set({ stage }),
  setPid: (pid) => set({ pid }),
  setProject: (project) => set({ project }),
  setProgress: (stage, msg) => set({ progress: { stage, msg } }),
  setRendered: (rendered) => set({ rendered }),
  setRendering: (rendering) => set({ rendering }),
  addExport: (e) => set((s) => ({ exports: [e, ...s.exports] })),
  updateExport: (id, patch) => set((s) => ({ exports: s.exports.map((x) => (x.id === id ? { ...x, ...patch } : x)) })),
  bump: () => set((s) => ({ rev: s.rev + 1 })),
}));
