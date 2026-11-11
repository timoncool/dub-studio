import { useTranslation } from "react-i18next";
import { ExternalLink, TriangleAlert } from "lucide-react";
import type { GpuReport } from "../lib/api";
import { useGpuReasonText } from "../lib/setupText";

// Предупреждение «Первого запуска» и менеджера моделей: локальные стадии на этой карте пойдут на CPU.
export default function GpuNotice({ gpu, driverUrl }: { gpu: GpuReport; driverUrl?: string | null }) {
  const { t } = useTranslation();
  const reasonText = useGpuReasonText();
  if (gpu.cuda13Ok || !gpu.reason) return null;
  const canUpdate = gpu.reason === "driver_old" || gpu.reason === "cuda_init" || gpu.reason === "no_nvidia";
  return (
    <div role="status" className="rounded-lg border border-[var(--color-warn)]/40 bg-[color-mix(in_oklab,var(--color-warn)_10%,transparent)] px-3 py-2.5 text-[12.5px] leading-snug">
      <div className="flex items-start gap-2">
        <TriangleAlert size={15} className="shrink-0 mt-0.5 text-[var(--color-warn)]" />
        <div className="min-w-0 flex-1">
          <div className="font-semibold text-[var(--color-warn)]">{t("downloads.gpuTitle")}</div>
          <div className="text-[var(--color-text)]">{reasonText(gpu)}</div>
        </div>
        {canUpdate && driverUrl && (
          <button onClick={() => window.open(driverUrl, "_blank")}
            className="shrink-0 inline-flex items-center gap-1 text-[12px] text-[var(--color-accent-2)] hover:underline">
            <ExternalLink size={13} />{t("downloads.updateDriver")}
          </button>
        )}
      </div>
    </div>
  );
}
