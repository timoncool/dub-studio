// Открыть настройки на нужном разделе из любого места окна: цель — «раздел» или «раздел:часть»
// (часть — блок раздела, к которому прокрутить, например "cloud:key").
export const OPEN_SETTINGS_EVENT = "dub:open-settings";

export const SETTINGS_SECTIONS = ["models", "cloud", "quality", "network", "interface", "agent", "privacy", "about"] as const;
export type SettingsSection = (typeof SETTINGS_SECTIONS)[number];

export type SettingsTarget = { section: SettingsSection; part: string | null };

export function openSettings(target: string): void {
  window.dispatchEvent(new CustomEvent<string>(OPEN_SETTINGS_EVENT, { detail: target }));
}

// Цель, которую окно настроек умеет открыть: раздел из `available`, иначе null (не молча первый раздел).
export function parseSettingsTarget(target: string | null | undefined, available: readonly SettingsSection[]): SettingsTarget | null {
  const [page, part] = (target ?? "").split(":");
  const section = available.find((s) => s === page);
  return section ? { section, part: part || null } : null;
}
