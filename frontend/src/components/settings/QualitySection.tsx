import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { AudioLines, Captions, Clock, Mic2, Rewind, Scissors, Sparkles, Star, Timer } from "lucide-react";
import { api } from "../../lib/api";
import SettingSwitch from "./SettingSwitch";
import { useChanged } from "../editorBridge";
import { SETTINGS_CHANGED } from "../../lib/mcpBridge";

// Тумблеры качества — слоты active.json. defaultOn: слот, которого ещё нет в файле, считается включённым.
const VOICE = [
  { key: "qc_asr", icon: Captions, labelKey: "qc.asr.label", hintKey: "qc.asr.hint", tipKey: "qc.asr.tip", defaultOn: false },
  { key: "qc_duration", icon: Clock, labelKey: "qc.duration.label", hintKey: "qc.duration.hint", tipKey: "qc.duration.tip", defaultOn: true },
  { key: "multitake", icon: Star, labelKey: "qc.multitake.label", hintKey: "qc.multitake.hint", tipKey: "qc.multitake.tip", defaultOn: false },
  { key: "speech_rate_on", icon: Sparkles, labelKey: "qc.speechRate.label", hintKey: "qc.speechRate.hint", tipKey: "qc.speechRate.tip", defaultOn: true },
  { key: "lead_into_silence", icon: Rewind, labelKey: "qc.leadIntoSilence.label", hintKey: "qc.leadIntoSilence.hint", tipKey: "qc.leadIntoSilence.tip", defaultOn: false },
  { key: "auto_shorten", icon: Scissors, labelKey: "qc.autoShorten.label", hintKey: "qc.autoShorten.hint", tipKey: "qc.autoShorten.tip", defaultOn: true },
  { key: "emo_ref_on", icon: Mic2, labelKey: "qc.emoRef.label", hintKey: "qc.emoRef.hint", tipKey: "qc.emoRef.tip", defaultOn: true },
  { key: "breath_on", icon: AudioLines, labelKey: "qc.breath.label", hintKey: "qc.breath.hint", tipKey: "qc.breath.tip", defaultOn: false },
] as const;

const DIAGNOSTICS = [
  { key: "bench", icon: Timer, labelKey: "settings.bench", hintKey: "settings.benchHint", tipKey: "settings.benchHint", defaultOn: false },
] as const;

type Row = (typeof VOICE)[number] | (typeof DIAGNOSTICS)[number];

type Selection = Record<string, unknown>;

const isOn = (sel: Selection, row: Row) => (row.defaultOn ? sel[row.key] !== "0" : sel[row.key] === "1");

export default function QualitySection() {
  const { t } = useTranslation();
  const [sel, setSel] = useState<Selection | null>(null);
  const [saving, setSaving] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reread, setReread] = useState(0);
  useChanged(SETTINGS_CHANGED, () => setReread((n) => n + 1));

  useEffect(() => {
    api.capabilities()
      .then((c) => setSel(c.selection ?? {}))
      .catch((e: unknown) => setError(t("prefs.loadFailed", { error: e instanceof Error ? e.message : String(e) })));
  }, [t, reread]);

  const toggle = async (row: Row) => {
    if (!sel || saving) return;
    const next = isOn(sel, row) ? "0" : "1";
    setSaving(row.key);
    setError(null);
    try {
      setSel(await api.setSelection(row.key, next));
    } catch (e) {
      setError(t("prefs.saveFailed", { error: e instanceof Error ? e.message : String(e) }));
    } finally {
      setSaving(null);
    }
  };

  const group = (title: string, rows: readonly Row[]) => (
    <section className="mb-5">
      <h4 className="text-[11px] uppercase tracking-[0.14em] text-[var(--color-muted)] mb-2">{title}</h4>
      <div className="rounded-lg border border-[var(--color-border)] divide-y divide-[var(--color-border)] bg-[var(--color-surface-2)]/40">
        {rows.map((row) => (
          <SettingSwitch key={row.key} icon={row.icon} label={t(row.labelKey)} hint={t(row.hintKey)} tip={t(row.tipKey)}
            on={sel ? isOn(sel, row) : row.defaultOn} busy={saving === row.key} disabled={!sel || saving !== null}
            onToggle={() => { void toggle(row); }} />
        ))}
      </div>
    </section>
  );

  return (
    <div className="max-w-2xl">
      {error && <p role="alert" className="mb-3 mono text-[11px] text-[var(--color-warn)] break-words">{error}</p>}
      {group(t("prefs.voiceGroup"), VOICE)}
      {group(t("prefs.diagnostics"), DIAGNOSTICS)}
    </div>
  );
}
