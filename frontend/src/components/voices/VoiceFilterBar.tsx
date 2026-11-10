import { RotateCcw, Star } from "lucide-react";
import { DURATIONS, NO_FILTER, narrowed, type VoiceFilter, type VoiceSort } from "../../lib/voiceFilter";

/** The search, filters and order of a voice list (the cloud catalogue, the library), one bar for every program that
 * lists voices. Keys it reads under `voices.`: searchAll, gender, gender_{all,female,male}, age, age_{all,adult,child},
 * lang, anyLanguage, source, anySource, minDuration, durationAny, durationMin, sortBy, sortName, sortDuration,
 * sortYoung, favorites, resetFilters. */
export default function VoiceFilterBar({ filter, onChange, languages, langLabel, sources, sourceLabel, favorites, shown, total, t }: {
  filter: VoiceFilter;
  onChange: (f: VoiceFilter) => void;
  languages: string[];
  langLabel: (code: string) => string;
  /** Where the voices came from; none — no source filter. */
  sources?: string[];
  sourceLabel?: (source: string) => string;
  /** Whether the list has favourites to keep to. */
  favorites?: boolean;
  shown: number;
  total: number;
  t: (key: string, options?: Record<string, unknown>) => string;
}) {
  const set = (patch: Partial<VoiceFilter>) => onChange({ ...filter, ...patch });
  const seg = (on: boolean) => `px-2.5 py-1.5 ${on ? "bg-[var(--color-accent)] text-[var(--color-on-accent)]" : "text-[var(--color-muted)] hover:text-[var(--color-text)]"}`;
  const sel = "shrink-0 bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-lg px-2 py-1.5 text-[12px] focus:border-[var(--color-accent)] focus:outline-none";
  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-2">
        <input value={filter.q} onChange={(e) => set({ q: e.target.value })} placeholder={t("voices.searchAll")} aria-label={t("voices.searchAll")} type="search"
          className="flex-1 min-w-[160px] bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-lg px-2.5 py-1.5 text-[13px] focus:border-[var(--color-accent)] focus:outline-none" />
        <div className="flex rounded-lg border border-[var(--color-border)] overflow-hidden text-[12px] shrink-0" role="group" aria-label={t("voices.gender")}>
          {(["all", "female", "male"] as const).map((g) => (
            <button key={g} type="button" onClick={() => set({ gender: g })} aria-pressed={filter.gender === g} className={seg(filter.gender === g)}>{t(`voices.gender_${g}`)}</button>
          ))}
        </div>
        <div className="flex rounded-lg border border-[var(--color-border)] overflow-hidden text-[12px] shrink-0" role="group" aria-label={t("voices.age")}>
          {(["all", "adult", "child"] as const).map((a) => (
            <button key={a} type="button" onClick={() => set({ age: a })} aria-pressed={filter.age === a} className={seg(filter.age === a)}>{t(`voices.age_${a}`)}</button>
          ))}
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <select value={filter.lang} onChange={(e) => set({ lang: e.target.value })} aria-label={t("voices.lang")} className={sel}>
          <option value="">{t("voices.anyLanguage")}</option>
          {languages.map((c) => <option key={c} value={c}>{langLabel(c)}</option>)}
        </select>
        {sources && sources.length > 0 && (
          <select value={filter.source} onChange={(e) => set({ source: e.target.value })} aria-label={t("voices.source")} className={sel}>
            <option value="">{t("voices.anySource")}</option>
            {sources.map((s) => <option key={s} value={s}>{sourceLabel ? sourceLabel(s) : s}</option>)}
          </select>
        )}
        <select value={filter.minDur} onChange={(e) => set({ minDur: Number(e.target.value) })} aria-label={t("voices.minDuration")} className={sel}>
          {DURATIONS.map((s) => <option key={s} value={s}>{s === 0 ? t("voices.durationAny") : t("voices.durationMin", { s })}</option>)}
        </select>
        <select value={filter.sort} onChange={(e) => set({ sort: e.target.value as VoiceSort })} aria-label={t("voices.sortBy")} className={sel}>
          <option value="name">{t("voices.sortName")}</option>
          <option value="duration">{t("voices.sortDuration")}</option>
          <option value="young">{t("voices.sortYoung")}</option>
        </select>
        {favorites && (
          <button type="button" onClick={() => set({ fav: !filter.fav })} aria-pressed={filter.fav}
            className={`shrink-0 inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg border text-[12px] ${filter.fav ? "border-[var(--color-warn)] text-[var(--color-warn)]" : "border-[var(--color-border)] text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
            <Star size={13} fill={filter.fav ? "currentColor" : "none"} />{t("voices.favorites")}
          </button>
        )}
        <span className="mono text-[11px] text-[var(--color-muted)]" aria-live="polite">{shown}/{total}</span>
        {narrowed(filter) && (
          <button type="button" onClick={() => onChange({ ...NO_FILTER, sort: filter.sort })}
            className="shrink-0 inline-flex items-center gap-1 text-[12px] text-[var(--color-accent)] hover:underline"><RotateCcw size={12} />{t("voices.resetFilters")}</button>
        )}
      </div>
    </div>
  );
}
