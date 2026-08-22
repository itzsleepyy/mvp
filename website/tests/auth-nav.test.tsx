import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AuthNav } from "@/components/auth-nav";

describe("auth navigation", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("reserves space while loading, then shows sign in for guests", async () => {
    let resolveFetch: (value: Response) => void = () => undefined;
    vi.stubGlobal(
      "fetch",
      vi.fn(
        () =>
          new Promise<Response>((resolve) => {
            resolveFetch = resolve;
          }),
      ),
    );
    const { container } = render(<AuthNav />);

    expect(container.querySelector("[aria-hidden='true']")).toHaveClass(
      "w-[4.5rem]",
    );
    resolveFetch(new Response(null, { status: 401 }));
    expect(await screen.findByRole("link", { name: "Sign in" })).toHaveAttribute(
      "href",
      "/sign-in",
    );
  });

  it("shows the user and clears the nav state on logout", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        Response.json({
          user: {
            id: "1",
            username: "octocat",
            display_name: "Octocat",
            avatar_url: null,
          },
        }),
      )
      .mockResolvedValueOnce(new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);
    render(<AuthNav />);

    expect(await screen.findByText("@octocat")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Sign out" }));
    await waitFor(() =>
      expect(screen.getByRole("link", { name: "Sign in" })).toBeInTheDocument(),
    );
    expect(fetchMock).toHaveBeenLastCalledWith("/api/auth/logout", {
      method: "POST",
    });
  });
});
