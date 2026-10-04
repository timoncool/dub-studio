import { describe, expect, it, vi } from "vitest";
import type { LaunchDefaults, LaunchDefaultsState } from "../src/lib/api";
import { createLaunchSaver, legacyPatch, loadWithMigration, type LaunchClient } from "../src/lib/launchDefaults";

const BUILTIN: LaunchDefaults = {
  speaker_count: 0,
  audio: "dub", subs: "translate", burn: true, detect_text: false, src_lang: "auto", tgt_lang: null,
  casting: false, casting_ref: "", content_type: "auto", vo_gain_db: -12, tr_style: "", tr_style_custom: "",
  sub_blur: true, keep_orig: false, container: "mp4", voice_src: "clone", voice_slots_m: [], voice_slots_f: [],
};

const store = (init: Record<string, string>) => {
  const data = new Map(Object.entries(init));
  return {
    data,
    getItem: (k: string) => data.get(k) ?? null,
    removeItem: (k: string) => { data.delete(k); },
  };
};

const client = (saved: boolean, reject = false) => {
  const saves: Partial<LaunchDefaults>[] = [];
  let state: LaunchDefaultsState = { defaults: BUILTIN, saved };
  const c: LaunchClient = {
    load: async () => state,
    save: async (patch) => {
      if (reject) throw new Error("400 vo_gain_db");
      saves.push(patch);
      state = { defaults: { ...state.defaults, ...patch }, saved: true };
      return state;
    },
  };
  return { c, saves };
};

describe("the old launch choice of the window", () => {
  it("is read as the start screen read it before", () => {
    const { patch, keys } = legacyPatch(store({
      "dub-casting": "1", "dub-content-type": "cartoon", "dub-vo-gain": "-40", "dub-tr-style-choice": "custom",
      "dub-container": "mkv", "dub-voice-slots-m": "[\"RU_Male_A\", 3]", "dub-voice-slots-f": "not json", "dub-casting-ref": "../x",
    }));
    expect(patch).toEqual({
      casting: true, content_type: "auto", vo_gain_db: -24, tr_style: "custom", container: "mkv",
      voice_slots_m: ["RU_Male_A"], voice_slots_f: [], casting_ref: "",
    });
    expect(keys.sort()).toEqual(["dub-casting", "dub-casting-ref", "dub-container", "dub-content-type", "dub-tr-style-choice", "dub-vo-gain", "dub-voice-slots-f", "dub-voice-slots-m"]);
  });

  it("moves to the service once and leaves the window only after the service took it", async () => {
    const s = store({ "dub-keep-orig": "1", "dub-sub-blur": "0", lang: "ru" });
    const { c, saves } = client(false);
    const got = await loadWithMigration(c, s);
    expect(saves).toEqual([{ keep_orig: true, sub_blur: false }]);
    expect(got.defaults.keep_orig).toBe(true);
    expect([...s.data.keys()]).toEqual(["lang"]);
  });

  it("stays in the window when the service refuses it", async () => {
    const s = store({ "dub-vo-gain": "-6" });
    const { c } = client(false, true);
    await expect(loadWithMigration(c, s)).rejects.toThrow("400");
    expect(s.data.get("dub-vo-gain")).toBe("-6");
  });

  it("is dropped when the service already keeps the defaults", async () => {
    const s = store({ "dub-casting": "1" });
    const { c, saves } = client(true);
    const got = await loadWithMigration(c, s);
    expect(saves).toEqual([]);
    expect(got.defaults.casting).toBe(false);
    expect(s.data.size).toBe(0);
  });
});

describe("saving the form", () => {
  it("sends the changes of a pause as one patch, in order", async () => {
    vi.useFakeTimers();
    const sent: Partial<LaunchDefaults>[] = [];
    const saver = createLaunchSaver(async (p) => { sent.push(p); return { defaults: BUILTIN, saved: true }; }, () => { throw new Error("unexpected"); }, 300);
    saver.queue({ audio: "voiceover" });
    saver.queue({ vo_gain_db: -6 });
    saver.queue({ speaker_count: 8 });
    saver.queue({ audio: "nodub" });
    await vi.advanceTimersByTimeAsync(300);
    saver.queue({ burn: false });
    await saver.flush();
    expect(sent).toEqual([{ audio: "nodub", vo_gain_db: -6, speaker_count: 8 }, { burn: false }]);
    vi.useRealTimers();
  });

  it("reports a failed save", async () => {
    const errors: unknown[] = [];
    const saver = createLaunchSaver(async () => { throw new Error("400 tgt_lang"); }, (e) => errors.push(e), 300);
    saver.queue({ tgt_lang: "xx" });
    await saver.flush();
    expect(String(errors[0])).toContain("tgt_lang");
  });
});
