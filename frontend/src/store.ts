import { create } from "zustand";
import type { Project } from "./lib/api";

type Stage = "empty" | "analyzing" | "editor";

type State = {
  stage: Stage;
  pid: string | null;
  project: Project | null;
  progress: { stage: string; msg: string };
  rendered: boolean;
  setStage: (s: Stage) => void;
  setPid: (p: string | null) => void;
  setProject: (p: Project | null) => void;
  setProgress: (stage: string, msg: string) => void;
  setRendered: (b: boolean) => void;
};

export const useStore = create<State>((set) => ({
  stage: "empty",
  pid: null,
  project: null,
  progress: { stage: "", msg: "" },
  rendered: false,
  setStage: (stage) => set({ stage }),
  setPid: (pid) => set({ pid }),
  setProject: (project) => set({ project }),
  setProgress: (stage, msg) => set({ progress: { stage, msg } }),
  setRendered: (rendered) => set({ rendered }),
}));
