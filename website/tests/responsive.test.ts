import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

describe("responsive foundation", () => {
  it("defines a mobile-safe layout without horizontal overflow", () => {
    const css = readFileSync(resolve(process.cwd(), "app/globals.css"), "utf8");
    expect(css).toContain("min-width: 320px");
    expect(css).toContain("overflow-x: hidden");
    expect(css).toContain("@media (prefers-reduced-motion: reduce)");
    expect(css).toContain('@import "tailwindcss"');
  });

  it("exposes the brand pixel font as a utility", () => {
    const css = readFileSync(resolve(process.cwd(), "app/globals.css"), "utf8");
    expect(css).toContain("--font-pixelify");
  });
});
