import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, Download, Loader2, Pause, Play, X } from "lucide-react";
import { api, type CatalogVoice } from "../../lib/api";
import { named } from "../../lib/a11y";
import { useEscapeLayer } from "../../lib/escape";
import { errorText, langName, prettyVoice } from "../../lib/voices";
import { applyVoiceFilter, NO_FILTER, type VoiceFacts } from "../../lib/voiceFilter";
import { useStore } from "../../store";
import VoiceFilterBar from "./VoiceFilterBar";

/** The cloud voice catalogue (nerualdreming/russia_voices): load the list with its metadata, filter and search,
 * listen, pick, and download the picked or everything the filters show (its sample transcript comes with it when the
 * dataset has one; voicing hears the others). `onVoices` gets the library as it stands after a download. */
export default function VoiceCatalog({ have, onClose, onVoices }: { have: string[]; onClose: () => void; onVoices: (voices: string[]) => void }) {
  const { t } = useTranslation();
  const pushActivity = useStore((s) => s.pushActivity);
  const [all, setAll] = useState<CatalogVoice[] | null>(null);
  const [filter, setFilter] = useState(NO_FILTER);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [playing, setPlaying] = useState<string | null>(null);
  const [getting, setGetting] = useState<{ done: number; total: number; current: string } | null>(null);
  const [problems, setProblems] = useState<string[]>([]);
  const audio = useRef<HTMLAudioElement | null>(null);
  const stopRef = useRef(false);

  useEffect(() => {
    api.voicesCatalog().then((r) => setAll(r.voices), (e: unknown) => { setAll([]); setProblems([t("voice.catalogFailed", { error: errorText(e) })]); });
    return () => { audio.current?.pause(); stopRef.current = true; };
  }, [t]);
  useEscapeLayer(() => { if (!getting) onClose(); });

  const languages = useMemo(() => {
    const codes = new Set<string>();
    for (const v of all ?? []) if (v.language) codes.add(v.language);
    return [...codes].sort((a, b) => langName(a).localeCompare(langName(b)));
  }, [all]);

  const owned = (n: string) => have.includes(n) || have.includes(n.split("/").pop() || n);
  const name = (v: CatalogVoice) => v.displayName || prettyVoice(v.name);
  const play = (v: CatalogVoice) => {
    audio.current?.pause();
    if (playing === v.name) { setPlaying(null); return; }
    const a = new Audio(v.url);
    audio.current = a;
    setPlaying(v.name);
    a.onended = () => setPlaying(null);
    a.play().catch((e: unknown) => { setPlaying(null); setProblems([errorText(e)]); });
  };

  const filtered = useMemo(() => {
    const label = (v: CatalogVoice) => v.displayName || prettyVoice(v.name);
    const facts = (v: CatalogVoice): VoiceFacts => ({
      label: label(v), gender: v.gender ?? "", age: v.age_group ?? "", lang: v.language ?? "", description: v.description ?? "",
      tags: [v.name], source: "", favorite: false, duration: v.duration_s ?? null, born: v.birth_year ?? null,
      search: [label(v), v.description ?? "", v.name, v.language ?? "", langName(v.language ?? ""),
        v.gender ? t(`voices.gender_${v.gender as "female" | "male"}`) : "", v.age_group ? t(`voices.age_${v.age_group as "adult" | "child"}`) : "",
        v.birth_year ? String(v.birth_year) : ""].filter(Boolean).join(" "),
    });
    return applyVoiceFilter(all ?? [], facts, filter);
  }, [all, filter, t]);

  const toggle = (n: string) => setPicked((p) => { const s = new Set(p); if (s.has(n)) s.delete(n); else s.add(n); return s; });

  const download = async (names: string[]) => {
    const todo = names.filter((n) => !owned(n));
    if (!todo.length) return;
    setProblems([]);
    stopRef.current = false;
    const failed: string[] = [];
    const got: string[] = [];
    const shown = (n: string) => { const v = (all ?? []).find((e) => e.name === n); return v ? name(v) : prettyVoice(n); };
    for (let i = 0; i < todo.length; i++) {
      if (stopRef.current) break;
      setGetting({ done: i, total: todo.length, current: shown(todo[i]) });
      try {
        const r = await api.voicesGet(todo[i]);
        if (r.ok) {
          got.push(shown(todo[i]));
          if (r.voices) onVoices(r.voices);
        } else failed.push(t("voice.getFailed", { name: shown(todo[i]), error: r.error ?? "" }));
      } catch (e) { failed.push(t("voice.getFailed", { name: shown(todo[i]), error: errorText(e) })); }
    }
    setGetting(null);
    setPicked(new Set());
    setProblems(failed);
    if (got.length) pushActivity(t("voices.imported", { names: got.join(", ") }), "done");
  };

  return (
    <div className="fixed inset-0 z-[70] grid place-items-center glass-scrim anim-fade" onClick={() => { if (!getting) onClose(); }}>
      <div role="dialog" aria-modal="true" aria-labelledby="catalog-title" className="w-[min(94vw,680px)] h-[min(86vh,740px)] flex flex-col rounded-xl glass-panel anim-pop p-5" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between mb-1">
          <span id="catalog-title" className="font-semibold">{t("voices.catalogTitle")}</span>
          <button onClick={onClose} disabled={!!getting} {...named(t("a11y.close"))} className="text-[var(--color-muted)] hover:text-[var(--color-text)] disabled:opacity-30"><X size={16} /></button>
        </div>
        <p className="text-[12px] text-[var(--color-muted)] mb-3">{t("voices.catalogHint")}</p>
        <div className="mb-3">
          <VoiceFilterBar filter={filter} onChange={setFilter} languages={languages} langLabel={langName} shown={filtered.length} total={all?.length ?? 0} t={t} />
        </div>
        {problems.length > 0 && <div role="alert" className="mb-2 max-h-20 overflow-y-auto text-[12px] text-[var(--color-warn)] break-words space-y-0.5">{problems.map((p, i) => <div key={i}>{p}</div>)}</div>}
        <div className="flex-1 overflow-y-auto -mr-2 pr-2 space-y-1">
          {all === null ? <div className="grid place-items-center py-10"><Loader2 size={18} className="animate-spin text-[var(--color-muted)]" /></div> :
            filtered.slice(0, 400).map((v) => {
              const has = owned(v.name);
              return (
                <label key={v.name} className={`flex items-center gap-2 px-2.5 py-1.5 rounded-lg border ${picked.has(v.name) ? "border-[var(--color-accent)] bg-[color-mix(in_oklab,var(--color-accent)_8%,transparent)]" : "border-[var(--color-border)] bg-[var(--color-surface-2)]"} ${has ? "" : "cursor-pointer"}`}>
                  {has ? <Check size={14} className="text-[var(--color-accent)] shrink-0" aria-label={t("voices.have")} /> :
                    <input type="checkbox" checked={picked.has(v.name)} onChange={() => toggle(v.name)} disabled={!!getting} className="w-3.5 h-3.5 accent-[var(--color-accent)] shrink-0" />}
                  <span className="text-[11px] text-[var(--color-muted)] w-4 shrink-0" aria-label={t(`voices.gender_${v.gender === "female" ? "female" : "male"}`)}>{v.gender === "female" ? "♀" : "♂"}</span>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-1.5">
                      <span className="text-[13px] truncate">{name(v)}</span>
                      {v.language && <span className="mono text-[10px] text-[var(--color-muted)] shrink-0">{v.language.toUpperCase()}</span>}
                      {v.age_group === "child" && <span className="text-[10px] text-[var(--color-accent-2)] shrink-0">{t("voices.age_child")}</span>}
                      {typeof v.duration_s === "number" && <span className="mono text-[10px] text-[var(--color-muted)] shrink-0">{t("voices.seconds", { s: Math.round(v.duration_s) })}</span>}
                      {typeof v.birth_year === "number" && <span className="mono text-[10px] text-[var(--color-muted)] shrink-0">{t("voices.born", { year: v.birth_year })}</span>}
                    </div>
                    {v.description && <div className="text-[11px] text-[var(--color-muted)] truncate" title={v.description}>{v.description}</div>}
                  </div>
                  <button type="button" onClick={(e) => { e.preventDefault(); play(v); }} {...named(t("voice.preview"))}
                    className="shrink-0 p-1 rounded-md text-[var(--color-muted)] hover:text-[var(--color-accent)]">{playing === v.name ? <Pause size={14} /> : <Play size={14} />}</button>
                </label>
              );
            })}
          {all !== null && all.length > 0 && filtered.length === 0 && (
            <div className="text-center py-10 space-y-2">
              <p className="text-[12px] text-[var(--color-muted)]">{t("voices.nothingFound")}</p>
              <button type="button" onClick={() => setFilter({ ...NO_FILTER, sort: filter.sort })} className="text-[12px] text-[var(--color-accent)] hover:underline">{t("voices.resetFilters")}</button>
            </div>
          )}
          {all !== null && filtered.length > 400 && <div className="mono text-[10px] text-[var(--color-muted)] text-center py-2">{t("voice.narrowSearch")}</div>}
        </div>
        <div className="mt-3 flex items-center gap-2">
          {getting ? (
            <>
              <Loader2 size={14} className="animate-spin text-[var(--color-accent)]" />
              <span className="text-[12px] text-[var(--color-muted)] flex-1 truncate">{t("voices.downloading", { done: getting.done + 1, total: getting.total, name: getting.current })}</span>
              <button onClick={() => { stopRef.current = true; }} className="px-3 py-1.5 rounded-lg border border-[var(--color-border)] text-[12px] hover:border-[var(--color-accent)]">{t("voices.stop")}</button>
            </>
          ) : (
            <>
              <span className="text-[12px] text-[var(--color-muted)] flex-1">{t("voices.pickedN", { n: picked.size })}</span>
              <button onClick={() => download(filtered.map((v) => v.name))} disabled={!filtered.length}
                className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-[var(--color-border)] text-[12px] hover:border-[var(--color-accent)] disabled:opacity-40"><Download size={13} />{t("voices.downloadAllShown")}</button>
              <button onClick={() => download([...picked])} disabled={!picked.size}
                className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-[12px] font-semibold disabled:opacity-40"><Download size={13} />{t("voices.downloadPicked")}</button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
