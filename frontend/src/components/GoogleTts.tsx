import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Trash2 } from "lucide-react";
import { api, type OpenRouterSettings } from "../lib/api";

export function GoogleKey({ onSaved }: { onSaved: () => void }) {
  const { t } = useTranslation();
  const [state, setState] = useState<OpenRouterSettings | null>(null);
  const [key, setKey] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => { api.googleSettings().then(setState).catch((e: unknown) => setError(String(e))); }, []);
  const run = async (remove: boolean) => {
    setBusy(true); setError("");
    try {
      setState(await (remove ? api.deleteGoogleKey() : api.saveGoogleKey(key.trim())));
      setKey(""); onSaved();
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    setBusy(false);
  };
  const locked = state?.source === "environment";
  return <div className="px-2.5 py-2 rounded-lg bg-[var(--color-surface-2)] border border-[var(--color-border)] space-y-2">
    <div className="text-[12px] font-medium">{t("google.keyTitle")}</div>
    <div className="text-[11px] text-[var(--color-muted)]">{t("google.keyHint")}</div>
    <div className="flex gap-2">
      <input type="password" data-mcp-secret autoComplete="off" spellCheck={false} value={key} onChange={e => setKey(e.target.value)} disabled={locked || !state || busy}
        aria-label={t("google.keyTitle")} placeholder={locked ? t("google.envKey",{name:state.environment_variable}) : state?.configured ? t("google.savedKey") : t("google.keyPlaceholder")}
        className="flex-1 min-w-0 px-2 py-1 rounded-md bg-[var(--color-surface)] border border-[var(--color-border)] text-[12px] mono" />
      <button onClick={() => run(false)} disabled={busy || locked || !key.trim()} className="px-2 py-1 border border-[var(--color-border)] rounded-md text-[12px] disabled:opacity-40">{t("google.verifySave")}</button>
      {state?.configured && !locked && <button onClick={() => run(true)} disabled={busy} aria-label={t("google.deleteKey")} className="px-2 border border-[var(--color-border)] rounded-md"><Trash2 size={13}/></button>}
    </div>
    {error && <div role="alert" className="text-[11px] text-[var(--color-warn)]">{t("google.error",{detail:error})}</div>}
  </div>;
}

export default function GoogleTts({model,mode,onChange}: {model:string;mode:string;onChange:(key:string,value:string)=>void}) {
  const { t } = useTranslation();
  const [models,setModels] = useState<{name:string;displayName?:string;supportedGenerationMethods?:string[]}[]>([]);
  const [error,setError] = useState("");
  useEffect(() => { api.googleModels().then(r=>setModels(r.models)).catch((e:unknown)=>setError(e instanceof Error ? e.message : String(e))); },[]);
  const choice = models.find(m=>m.name === `models/${model}`);
  const supportsBatch = choice?.supportedGenerationMethods?.includes("batchGenerateContent");
  return <div className="space-y-2">
    <label className="block text-[12px]">{t("google.model")}
      <select value={model} onChange={e=>onChange("google_tts_model",e.target.value)} className="block w-full mt-1 bg-[var(--color-surface)] border border-[var(--color-border)] rounded-md p-1">
        <option value="">{t("google.pickModel")}</option>
        {model && !choice && <option value={model}>{model}</option>}
        {models.map(m=><option key={m.name} value={m.name.replace(/^models\//,"")}>{m.displayName ?? m.name}</option>)}
      </select>
    </label>
    <label className="block text-[12px]">{t("google.mode")}
      <select value={mode || "standard"} onChange={e=>onChange("google_tts_mode",e.target.value)} className="block w-full mt-1 bg-[var(--color-surface)] border border-[var(--color-border)] rounded-md p-1">
        <option value="standard">{t("google.standard")}</option>
        <option value="batch" disabled={!!choice && !supportsBatch}>{t("google.batch")}</option>
      </select>
    </label>
    <p className="text-[11px] text-[var(--color-muted)]">{t(mode === "batch" ? "google.batchHint" : "google.standardHint")}</p>
    {error && <div role="alert" className="text-[11px] text-[var(--color-warn)]">{t("google.error",{detail:error})}</div>}
  </div>;
}
