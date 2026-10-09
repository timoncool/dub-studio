import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Boxes, Cloud, Gauge, Globe, Info, Languages, Plug, X, type LucideIcon, BarChart3 } from "lucide-react";
import { parseSettingsTarget, type SettingsSection } from "../../lib/settingsNav";
import { useEscapeLayer } from "../../lib/escape";
import QualitySection from "./QualitySection";
import InterfaceSection from "./InterfaceSection";
import AboutSection from "./AboutSection";
import PrivacySection from "./PrivacySection";

// Разделы, которые окно настроек получает от места подключения: их компоненты живут рядом со своим
// состоянием (модели, облако, сеть). agent появляется, когда передана его панель.
export type SettingsPanes = { models: ReactNode; cloud: ReactNode; network: ReactNode; agent?: ReactNode };

const SECTIONS: { id: SettingsSection; icon: LucideIcon }[] = [
  { id: "models", icon: Boxes },
  { id: "cloud", icon: Cloud },
  { id: "quality", icon: Gauge },
  { id: "network", icon: Globe },
  { id: "interface", icon: Languages },
  { id: "agent", icon: Plug },
  { id: "privacy", icon: BarChart3 },
  { id: "about", icon: Info },
];

const LABEL = {
  models: ["prefs.sections.models", "prefs.sections.modelsHint"],
  cloud: ["prefs.sections.cloud", "prefs.sections.cloudHint"],
  quality: ["prefs.sections.quality", "prefs.sections.qualityHint"],
  network: ["prefs.sections.network", "prefs.sections.networkHint"],
  interface: ["prefs.sections.interface", "prefs.sections.interfaceHint"],
  agent: ["prefs.sections.agent", "prefs.sections.agentHint"],
  privacy: ["prefs.sections.privacy", "prefs.sections.privacyHint"],
  about: ["prefs.sections.about", "prefs.sections.aboutHint"],
} as const satisfies Record<SettingsSection, readonly [string, string]>;

// Настройки разделами: навигация слева (на узком окне — ряд чипов), шапка активного раздела, Escape и клик
// по подложке закрывают. target — «раздел» или «раздел:часть»; часть — блок с data-settings-part, к нему
// окно прокручивает и ставит фокус. request меняется с каждым новым запросом открыть цель, даже ту же самую.
export default function SettingsModal({ target, request, panes, onClose }: { target: string; request: number; panes: SettingsPanes; onClose: () => void }) {
  const { t } = useTranslation();
  const available = SECTIONS.filter((s) => s.id !== "agent" || panes.agent !== undefined);
  const ids = available.map((s) => s.id);
  const initial = parseSettingsTarget(target, ids);
  const [section, setSection] = useState<SettingsSection>(initial?.section ?? "models");
  const [focus, setFocus] = useState<{ part: string } | null>(initial?.part ? { part: initial.part } : null);
  const [seenRequest, setSeenRequest] = useState(request);
  const body = useRef<HTMLDivElement>(null);
  const titleId = useId();

  if (request !== seenRequest) {
    setSeenRequest(request);
    const next = parseSettingsTarget(target, ids);
    if (next) { setSection(next.section); setFocus(next.part ? { part: next.part } : null); }
  }

  useEscapeLayer(onClose);

  // Раздел может дорисоваться позже (ждёт ответа сервера): ждём появления блока, а не один кадр.
  useEffect(() => {
    const root = body.current;
    if (!focus || !root) return;
    const reveal = () => {
      const el = root.querySelector<HTMLElement>(`[data-settings-part="${focus.part}"]`);
      if (!el) return false;
      el.scrollIntoView({ block: "center" });
      el.querySelector<HTMLElement>("input, select, textarea, button")?.focus();
      return true;
    };
    if (reveal()) return;
    const watch = new MutationObserver(() => { if (reveal()) watch.disconnect(); });
    watch.observe(root, { childList: true, subtree: true });
    return () => watch.disconnect();
  }, [focus, section]);

  const choose = (id: SettingsSection) => { setSection(id); setFocus(null); };
  const [label, hint] = LABEL[section];

  return (
    <div className="fixed inset-0 z-50 grid place-items-center glass-scrim anim-fade" onClick={onClose}>
      <div role="dialog" aria-modal="true" aria-labelledby={titleId}
        className="w-[min(94vw,900px)] h-[min(88vh,760px)] flex rounded-xl glass-panel anim-pop overflow-hidden" onClick={(e) => e.stopPropagation()}>
        <nav aria-label={t("prefs.title")} className="hidden sm:flex w-56 shrink-0 flex-col gap-1 border-r border-[var(--color-border)] p-3 overflow-y-auto">
          <h2 className="px-2 pt-1 pb-2 font-semibold">{t("prefs.title")}</h2>
          {available.map(({ id, icon: Icon }) => (
            <button key={id} type="button" onClick={() => choose(id)} aria-current={section === id ? "page" : undefined}
              className={`flex w-full items-start gap-2.5 rounded-lg px-2.5 py-2 text-left transition-colors ${section === id ? "bg-[var(--color-surface-2)] text-[var(--color-text)] shadow-[inset_2px_0_0_var(--color-accent)]" : "text-[var(--color-muted)] hover:bg-[var(--color-surface-2)]/60 hover:text-[var(--color-text)]"}`}>
              <Icon size={15} className={`mt-0.5 shrink-0 ${section === id ? "text-[var(--color-accent)]" : ""}`} />
              <span className="min-w-0">
                <span className="block truncate text-[13px] font-medium">{t(LABEL[id][0])}</span>
                <span className="block truncate text-[10px] text-[var(--color-muted)]">{t(LABEL[id][1])}</span>
              </span>
            </button>
          ))}
        </nav>

        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex items-center justify-between gap-3 border-b border-[var(--color-border)] px-5 py-3.5">
            <div className="min-w-0">
              <h3 id={titleId} className="truncate font-semibold">{t(label)}</h3>
              <p className="truncate text-[11px] text-[var(--color-muted)]">{t(hint)}</p>
            </div>
            <button type="button" onClick={onClose} aria-label={t("prefs.close")} title={t("prefs.close")}
              className="shrink-0 p-1.5 rounded-md text-[var(--color-muted)] hover:text-[var(--color-text)] hover:bg-[var(--color-surface-2)] transition-colors"><X size={16} /></button>
          </div>

          <div className="flex gap-1 overflow-x-auto border-b border-[var(--color-border)] px-3 py-2 sm:hidden">
            {available.map(({ id }) => (
              <button key={id} type="button" onClick={() => choose(id)} aria-current={section === id ? "page" : undefined}
                className={`shrink-0 rounded-full px-3 py-1 text-[11px] font-medium border ${section === id ? "border-[var(--color-accent)] text-[var(--color-text)] bg-[var(--color-surface-2)]" : "border-[var(--color-border)] text-[var(--color-muted)]"}`}>
                {t(LABEL[id][0])}
              </button>
            ))}
          </div>

          <div ref={body} className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
            {section === "models" && panes.models}
            {section === "cloud" && panes.cloud}
            {section === "quality" && <QualitySection />}
            {section === "network" && panes.network}
            {section === "interface" && <InterfaceSection />}
            {section === "agent" && panes.agent}
            {section === "privacy" && <PrivacySection />}
            {section === "about" && <AboutSection />}
          </div>

          <div className="flex justify-end border-t border-[var(--color-border)] px-5 py-3">
            <button type="button" onClick={onClose}
              className="px-5 py-1.5 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-[13px] font-semibold hover:brightness-105 transition">
              {t("prefs.done")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
