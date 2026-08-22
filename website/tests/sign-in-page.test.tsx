import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("@/components/sign-in-form", () => ({
  SignInForm: ({
    browserToken,
    cliComplete,
  }: {
    browserToken?: string;
    cliComplete?: boolean;
  }) => (
    <div
      data-testid="sign-in-form"
      data-browser-token={browserToken}
      data-cli-complete={String(cliComplete)}
    />
  ),
}));

import SignInPage from "@/app/sign-in/page";

describe("sign-in page", () => {
  it("passes a valid handoff from asynchronous search params", async () => {
    render(
      await SignInPage({
        searchParams: Promise.resolve({
          handoff: "browser-token-with-enough-length",
        }),
      }),
    );

    expect(screen.getByTestId("sign-in-form")).toHaveAttribute(
      "data-browser-token",
      "browser-token-with-enough-length",
    );
  });

  it("drops malformed or oversized handoff values", async () => {
    render(
      await SignInPage({
        searchParams: Promise.resolve({ handoff: "not valid!" }),
      }),
    );

    expect(screen.getByTestId("sign-in-form")).not.toHaveAttribute(
      "data-browser-token",
    );
  });

  it("passes the CLI completion state without requiring a handoff token", async () => {
    render(
      await SignInPage({
        searchParams: Promise.resolve({ cli: "complete" }),
      }),
    );

    expect(screen.getByTestId("sign-in-form")).toHaveAttribute(
      "data-cli-complete",
      "true",
    );
  });
});
