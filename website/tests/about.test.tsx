import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import AboutPage from "@/app/about/page";
import { Navbar } from "@/components/navbar";

describe("about page", () => {
  it("describes MVP and the three games", () => {
    render(<AboutPage />);

    expect(
      screen.getByRole("heading", { name: "About MVP" }),
    ).toBeInTheDocument();
    for (const title of ["Stack Overflow", "The Daily PR", "The Daily Fix"]) {
      expect(screen.getByRole("heading", { name: title })).toBeInTheDocument();
    }
    expect(screen.getByText("npm install mvp")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy" })).toBeInTheDocument();
  });

  it("lists supported coding agents", () => {
    render(<AboutPage />);

    for (const agent of ["Claude Code", "Codex", "Gemini CLI", "OpenCode"]) {
      expect(screen.getByText(agent)).toBeInTheDocument();
    }
  });

  it("mentions the online leaderboard and privacy boundary", () => {
    render(<AboutPage />);

    expect(screen.getByText(/Daily, Weekly, and All-Time/i)).toBeInTheDocument();
    expect(
      screen.getByText(/never includes source code, prompts/i),
    ).toBeInTheDocument();
  });

  it("links to about from the header", () => {
    render(<Navbar />);

    expect(screen.getByRole("link", { name: "About" })).toHaveAttribute(
      "href",
      "/about",
    );
  });
});
