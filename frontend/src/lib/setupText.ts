import { useTranslation } from "react-i18next";
import type { GpuReport } from "./api";

// Текст ошибки закачки по коду сервера (подробность — отдельной строкой для журнала).
export function useDownloadErrorText() {
  const { t } = useTranslation();
  const byCode: Record<string, string> = {
    disk_space: t("downloads.errDiskSpace"),
    hash_mismatch: t("downloads.errHash"),
    size_mismatch: t("downloads.errSize"),
    rate_limited: t("downloads.errRateLimited"),
    network: t("downloads.errNetwork"),
    http_status: t("downloads.errHttp"),
    io: t("downloads.errIo"),
    extract: t("downloads.errExtract"),
    proxy: t("downloads.errProxy"),
    busy: t("downloads.errBusy"),
    nothing_to_download: t("downloads.errNothing"),
    not_removable: t("downloads.errNotRemovable"),
    open_failed: t("downloads.errOpenFolder"),
  };
  return (code?: string | null) => (code && byCode[code]) || t("downloads.errGeneric");
}

// Почему GPU-стадии недоступны (драйвер старше CUDA 13, карта старше Turing, нет NVIDIA) — текстом по коду.
export function useGpuReasonText() {
  const { t } = useTranslation();
  return (gpu: GpuReport): string => {
    const byReason: Record<string, string> = {
      no_nvidia: t("downloads.gpuNoNvidia"),
      no_device: t("downloads.gpuNoDevice"),
      cuda_init: t("downloads.gpuCudaInit"),
      driver_old: t("downloads.gpuDriverOld", { driver: gpu.driverVersion ?? "?", cuda: gpu.cudaDriver ?? "?", min: gpu.minDriver }),
      gpu_old: t("downloads.gpuTooOld", { name: gpu.name ?? "GPU", cc: gpu.compute ?? "?", min: gpu.minCompute }),
      gpu_missing: t("downloads.gpuMissing", { index: gpu.chosen ?? "?", cards: gpu.cards.map((c) => `${c.index}: ${c.name}`).join(", ") || "—" }),
    };
    return (gpu.reason && byReason[gpu.reason]) || "";
  };
}
