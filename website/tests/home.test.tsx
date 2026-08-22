import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import Home from "@/app/page";
import { Navbar } from "@/components/navbar";

describe("homepage", () => {
  afterEach(() => vi.restoreAllMocks());

  it("explains MVP and renders the real install path", () => {
    render(<Home />);

    expect(
      screen.getByRole("heading", { name: /the arcade for programmers/i }),
    ).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /download mvp/i })).toHaveAttribute(
      "href",
      "#install",
    );
    expect(screen.getByText(/cargo install --path package/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy" })).toBeInTheDocument();
  });

  it("copies the installation command", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    render(<Home />);

    fireEvent.click(screen.getByRole("button", { name: "Copy" }));
    expect(writeText).toHaveBeenCalledWith(
      expect.stringContaining("github.com/itzsleepyy/waitstate"),
    );
    expect(await screen.findByRole("button", { name: "Copied" })).toBeInTheDocument();
  });

  it("shows all real games and supported coding agents", () => {
    render(<Home />);

    for (const name of ["Stack Overflow", "The Daily PR", "The Daily Fix"]) {
      expect(screen.getAllByText(name).length).toBeGreaterThan(0);
    }
    for (const name of ["Claude Code", "Codex", "Gemini CLI", "OpenCode"]) {
      expect(screen.getByText(name)).toBeInTheDocument();
    }
  });

  it("keeps keyboard users able to bypass navigation", () => {
    render(
      <>
        <Navbar />
        <Home />
      </>,
    );
    expect(screen.getByRole("link", { name: "Skip to content" })).toHaveAttribute(
      "href",
      "#main-content",
    );
    expect(document.querySelector("main")).toHaveAttribute("id", "main-content");
  });
});
