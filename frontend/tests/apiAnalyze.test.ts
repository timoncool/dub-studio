import { afterEach, describe, expect, it, vi } from "vitest";
import { api } from "../src/lib/api";

afterEach(() => vi.unstubAllGlobals());

describe("число спикеров в запросе анализа", () => {
  it("передаёт восемь участников и остальные параметры анализа", async () => {
    const fetch = vi.fn(async () => new Response(JSON.stringify({ job_id: "test-job" }), { status: 200 }));
    vi.stubGlobal("fetch", fetch);
    await api.analyze("offline", "ru", "transcribe", "ru", "transcribe", "", false, false, false, "", false, "", "auto", false, {}, 8);
    const [url, init] = fetch.mock.calls[0] as unknown as [string, RequestInit];
    const query = new URL(url, window.location.href).searchParams;
    expect(query.get("speaker_count")).toBe("8");
    expect(query.get("mode")).toBe("transcribe");
    expect(query.get("src_lang")).toBe("ru");
    expect(init.method).toBe("POST");
  });

  it("старый вызов оставляет автоматическое определение", async () => {
    const fetch = vi.fn(async () => new Response(JSON.stringify({ job_id: "test-job" }), { status: 200 }));
    vi.stubGlobal("fetch", fetch);
    await api.analyze("offline", "ru");
    const [url] = fetch.mock.calls[0] as unknown as [string];
    expect(new URL(url, window.location.href).searchParams.get("speaker_count")).toBe("0");
  });
});
