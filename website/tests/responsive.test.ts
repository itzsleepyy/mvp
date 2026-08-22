import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

describe("responsive foundation", () => {
  it("defines a mobile layout without horizontal page overflow", () => {
    const css = readFileSync(resolve(process.cwd(), "app/globals.css"), "utf8");
    expect(css).toContain("min-width: 320px");
    expect(css).toContain("overflow-x: hidden");
    expect(css).toContain("@media (max-width: 760px)");
    expect(css).toContain("font-size: clamp(44px, 14vw, 72px)");
    expect(css).toContain("@media (prefers-reduced-motion: reduce)");
  });
});
