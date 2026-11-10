/** What a voice list filters and sorts by, the same for the cloud catalogue and the library. */
export type VoiceFacts = {
  label: string;
  gender: string;
  age: string;
  lang: string;
  description: string;
  tags: string[];
  source: string;
  favorite: boolean;
  duration: number | null;
  born: number | null;
  /** Everything the text box searches by: the name, description and tags, plus every aspect of the dataset the list
   * has — gender, age, language (its name), year of birth and source — each in the window's words, so a search like
   * "ребёнок", "arabic" or "2004" finds the voice. The caller builds it; empty falls back to name and description. */
  search?: string;
};

export type VoiceSort = "name" | "duration" | "young";

export type VoiceFilter = {
  q: string;
  gender: "all" | "female" | "male";
  age: "all" | "adult" | "child";
  lang: string;
  source: string;
  minDur: number;
  fav: boolean;
  sort: VoiceSort;
};

export const NO_FILTER: VoiceFilter = { q: "", gender: "all", age: "all", lang: "", source: "", minDur: 0, fav: false, sort: "name" };

export const DURATIONS = [0, 10, 30, 60] as const;

/** Whether anything narrows the list (the order does not). */
export const narrowed = (f: VoiceFilter) =>
  f.q.trim() !== "" || f.gender !== "all" || f.age !== "all" || f.lang !== "" || f.source !== "" || f.minDur > 0 || f.fav;

/** The items that pass the filter, in its order; a length or year the source does not give sorts last. */
export function applyVoiceFilter<T>(items: T[], facts: (item: T) => VoiceFacts, f: VoiceFilter): T[] {
  const needle = f.q.trim().toLowerCase();
  const rows = items.map((item) => ({ item, v: facts(item) })).filter(({ v }) =>
    (f.gender === "all" || v.gender === f.gender) &&
    (f.age === "all" || v.age === f.age) &&
    (!f.lang || v.lang === f.lang) &&
    (!f.source || v.source === f.source) &&
    (!f.fav || v.favorite) &&
    (f.minDur === 0 || (v.duration ?? 0) >= f.minDur) &&
    (!needle || (v.search ?? [v.label, v.description, ...v.tags].join(" ")).toLowerCase().includes(needle)));
  const last = (n: number | null, value: (n: number) => number) => (n === null ? Number.POSITIVE_INFINITY : value(n));
  rows.sort((a, b) =>
    f.sort === "duration" ? last(a.v.duration, (n) => -n) - last(b.v.duration, (n) => -n) || a.v.label.localeCompare(b.v.label)
    : f.sort === "young" ? last(a.v.born, (n) => -n) - last(b.v.born, (n) => -n) || a.v.label.localeCompare(b.v.label)
    : a.v.label.localeCompare(b.v.label));
  return rows.map((r) => r.item);
}
