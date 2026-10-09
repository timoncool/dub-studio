// Studio Hub в окне: анонимная статистика (выбор и что уходит) и новости хаба поверх вшитых.
import { useEffect, useState } from "react";
import { BASE } from "./api";

export interface HubButton { label: string; action: "url" | "dismiss" | "open"; url?: string; style: "primary" | "secondary" }
export interface HubContent { title: string; body: string; buttons: HubButton[] }
export interface HubItem {
  id: string;
  kind: "news" | "bar" | "popup";
  priority: number;
  date: string | null;
  content: Record<string, HubContent>;
  eligible: boolean;
}
export interface HubState {
  telemetry: { enabled: boolean; acknowledged: boolean; disabledByEnv: boolean; install: string | null };
  source: string | null;
  fetchedAt: number | null;
  lastError: string | null;
  items: HubItem[];
}

const CHANGED = "dub:hub-changed";

async function call<T>(path: string, init?: RequestInit): Promise<T> {
  const r = await fetch(`${BASE}${path}`, init);
  if (!r.ok) throw new Error(`${path}: HTTP ${r.status}`);
  return (await r.json()) as T;
}

export const hubChanged = (): void => { window.dispatchEvent(new Event(CHANGED)); };

export const setTelemetry = (enabled: boolean, acknowledge = false) =>
  call<HubState["telemetry"]>("/v1/hub/telemetry", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ enabled, acknowledge }) });
export const telemetryPreview = () => call<{ enabled: boolean; report: unknown }>("/v1/hub/telemetry/preview");
export const resetInstall = () => call<unknown>("/v1/hub/telemetry/reset", { method: "POST" });
export const refreshHub = () => call<unknown>("/v1/hub/refresh", { method: "POST" });

/** Состояние хаба для языка окна; перечитывается раз в минуту и после любого изменения выбора. */
export function useHubState(lang: string): HubState | null {
  const [state, setState] = useState<HubState | null>(null);
  useEffect(() => {
    let alive = true;
    const load = () => {
      call<HubState>(`/v1/hub/state?lang=${encodeURIComponent(lang)}`)
        .then((s) => { if (alive) setState(s); })
        .catch((e: Error) => console.warn("[hub]", e.message));
    };
    load();
    const timer = window.setInterval(load, 60_000);
    window.addEventListener(CHANGED, load);
    return () => { alive = false; window.clearInterval(timer); window.removeEventListener(CHANGED, load); };
  }, [lang]);
  return state;
}

export function hubText(item: HubItem, lang: string): HubContent | undefined {
  return item.content[lang] ?? item.content.en ?? Object.values(item.content)[0];
}
