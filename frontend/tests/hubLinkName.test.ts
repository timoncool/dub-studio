import { describe, expect, it } from "vitest";
import { linkName } from "../src/lib/studioHub";

// The same pairs are in the hub's tests (studio-hub test/rules.test.ts) and the hub client's: a link is one name everywhere.
describe("a link of a hub notice is named by its address", () => {
  it("gives the hub's names", () => {
    expect(linkName("https://github.com/timoncool/dub-studio")).toBe("udad9bf80");
    expect(linkName("https://timoncool.github.io/dub-studio/")).toBe("ub870f0d3");
    expect(linkName("")).toBe("u811c9dc5");
    expect(linkName("https://example.com/путь")).toBe("u0c74563b");
  });
});
