import { describe, expect, it } from "vitest";
import { applyTypograf } from "./typograf";

describe("applyTypograf", () => {
  it("turns hyphen dash and ellipsis into typography", () => {
    const out = applyTypograf("Это - тест...");
    expect(out).toBe("Это\u00a0— тест…");
    expect(out).not.toContain("&nbsp;");
    expect(out).not.toContain("&mdash;");
  });

  it("returns null for empty or oversized input", () => {
    expect(applyTypograf("   ")).toBeNull();
    expect(applyTypograf("x".repeat(2001))).toBeNull();
  });
});

