import { render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import AsciiLogo from "@/components/ascii-logo";

describe("ASCII logo", () => {
  afterEach(() => vi.restoreAllMocks());

  it("renders every row on the same fixed-width canvas", () => {
    vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: true })));

    const { container } = render(<AsciiLogo />);
    const rows = container.querySelectorAll(".ascii-logo-row");

    expect(rows).toHaveLength(16);
    for (const row of rows) {
      expect(row.querySelectorAll(".ascii-logo-cell")).toHaveLength(78);
    }
  });
});
