import { useCallback, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { X } from "lucide-react";
import { HUB_GRADIENTS, HUB_TEXT, hubMediaUrl, hubText, openHubTarget, reportNotice, stackTheme, type HubButton, type HubItem } from "../lib/studioHub";
import { useEscapeLayer } from "../lib/escape";
import { NewsBody } from "./WhatsNew";

/** A hub popup in the studio's dialog style: a header in the notice's gradient, its picture, the text and its buttons.
 * The cross, Esc and a click outside close it only when it may be closed. */
export default function HubPopup({ item, onClose }: { item: HubItem; onClose: () => void }) {
  const { t, i18n } = useTranslation();
  const content = hubText(item, i18n.language.split("-")[0]);
  const theme = stackTheme(item.theme, 0);
  useEffect(() => {
    reportNotice(item.id, "shown").catch((e: Error) => console.warn("[hub] popup shown not recorded:", e.message));
  }, [item.id]);
  const dismiss = useCallback(() => {
    reportNotice(item.id, "dismissed").catch((e: Error) => console.warn("[hub] popup close not recorded:", e.message));
    onClose();
  }, [item.id, onClose]);
  useEscapeLayer(dismiss, item.dismissible);
  if (!content) return null;
  const press = (button: HubButton, index: number) => {
    if (button.action === "dismiss") {
      dismiss();
      return;
    }
    reportNotice(item.id, "clicked", `b${index}`).catch((e: Error) => console.warn("[hub] popup click not recorded:", e.message));
    if (button.action === "url" && button.url) window.open(button.url, "_blank", "noreferrer");
    if (button.action === "open" && button.target) openHubTarget(button.target);
    onClose();
  };
  return (
    <div className="fixed inset-0 z-[80] grid place-items-center glass-scrim anim-fade" onClick={() => { if (item.dismissible) dismiss(); }}>
      <div role="dialog" aria-modal="true" aria-labelledby={`hub-${item.id}`} className="w-[min(92vw,460px)] max-h-[86vh] overflow-y-auto rounded-xl glass-panel anim-pop"
        onClick={(e) => {
          e.stopPropagation();
          if ((e.target as HTMLElement).closest("a")) reportNotice(item.id, "clicked", "link").catch((err: Error) => console.warn("[hub] popup link not recorded:", err.message));
        }}>
        <div className="flex items-center justify-between gap-3 px-5 py-3.5 rounded-t-xl" style={{ background: HUB_GRADIENTS[theme], color: HUB_TEXT[theme] }}>
          <h2 id={`hub-${item.id}`} className="text-[15px] font-semibold leading-snug">{content.title}</h2>
          {item.dismissible && (
            <button type="button" onClick={dismiss} aria-label={t("hub.close")} className="shrink-0 rounded-md p-0.5 opacity-85 hover:opacity-100"><X size={16} /></button>
          )}
        </div>
        {item.image && <img src={hubMediaUrl(item.image)} alt="" className="max-h-56 w-full object-cover" />}
        <div className="px-5 pb-1 pt-2"><NewsBody text={content.body} /></div>
        {item.ad?.label && <p className="px-5 pt-1 text-[11px] text-[var(--color-muted)]">{item.ad.erid ? t("hub.adErid", { erid: item.ad.erid }) : t("hub.ad")}</p>}
        {content.buttons.length > 0 && (
          <div className="flex flex-wrap justify-end gap-2 px-5 pb-5 pt-3">
            {content.buttons.map((button, index) => (
              <button key={index} type="button" onClick={() => press(button, index)}
                className={button.style === "primary" ? "px-3.5 py-1.5 rounded-lg text-[13px] font-semibold hover:opacity-90" : "px-3.5 py-1.5 rounded-lg text-[13px] border border-[var(--color-border)] text-[var(--color-text)] hover:bg-[var(--color-surface-2)]"}
                style={button.style === "primary" ? { background: HUB_GRADIENTS[theme], color: HUB_TEXT[theme] } : undefined}>
                {button.label}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
