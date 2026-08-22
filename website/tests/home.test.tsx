import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import Home from "@/app/page";
import { Navbar } from "@/components/navbar";
import { Footer } from "@/components/footer";

vi.mock("@/components/ascii-logo", () => ({
  default: () => <div data-testid="ascii-logo" />,
}));

describe("homepage", () => {
  afterEach(() => vi.restoreAllMocks());

  it("shows a minimal MVP hero with the install pill", () => {
    render(<Home />);

    expect(screen.getByRole("heading", { name: "MVP" })).toBeInTheDocument();
    expect(screen.getByTestId("ascii-logo")).toBeInTheDocument();
    expect(screen.getByText("npm install mvp")).toBeInTheDocument();
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
    expect(writeText).toHaveBeenCalledWith("npm install mvp");
    expect(
      await screen.findByRole("button", { name: /copied/i }),
    ).toBeInTheDocument();
  });

  it("keeps keyboard users able to bypass navigation", () => {
    const layout = readFileSync(resolve(process.cwd(), "app/layout.tsx"), "utf8");
    expect(layout).toContain('className="skip-link"');
    expect(layout).toContain('href="#main-content"');

    render(
      <>
        <Navbar />
        <Home />
      </>,
    );
    expect(document.querySelector("main")).toHaveAttribute("id", "main-content");
  });

  it("links to the leaderboard and repository from the header", () => {
    render(
      <>
        <Navbar />
        <Home />
        <Footer />
      </>,
    );

    expect(screen.getByRole("link", { name: "Leaderboard" })).toHaveAttribute(
      "href",
      "/leaderboard",
    );
    const githubLinks = screen.getAllByRole("link", { name: "GitHub" });
    expect(githubLinks).toHaveLength(2);
    for (const link of githubLinks) {
      expect(link).toHaveAttribute(
        "href",
        "https://github.com/itzsleepyy/waitstate",
      );
    }
  });
});
