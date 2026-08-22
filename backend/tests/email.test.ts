import { afterEach, describe, expect, it, vi } from "vitest";
import { cloudflareEmailProvider } from "../src/email.js";

describe("cloudflareEmailProvider", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("sends the Cloudflare REST request", async () => {
    const fetch = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ success: true }),
    });
    vi.stubGlobal("fetch", fetch);
    const provider = cloudflareEmailProvider("account/id", "api-secret");

    await provider.send({
      from: "login@waitstate.example",
      to: "player@example.com",
      subject: "Sign in",
      text: "Text",
      html: "<p>HTML</p>",
    });

    expect(fetch).toHaveBeenCalledOnce();
    const [url, init] = fetch.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(
      "https://api.cloudflare.com/client/v4/accounts/account%2Fid/email/sending/send",
    );
    expect(init.headers).toMatchObject({
      Authorization: "Bearer api-secret",
      "Content-Type": "application/json",
    });
    expect(typeof init.body).toBe("string");
    expect(JSON.parse(init.body as string)).toEqual({
      from: "login@waitstate.example",
      to: "player@example.com",
      subject: "Sign in",
      text: "Text",
      html: "<p>HTML</p>",
    });
  });

  it("throws only a safe status error for a failed request", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: false,
        status: 403,
        json: () => Promise.resolve({ success: false }),
      }),
    );
    const provider = cloudflareEmailProvider("account", "api-secret");
    await expect(
      provider.send({
        from: "from@example.com",
        to: "to@example.com",
        subject: "Subject",
        text: "secret-link",
        html: "secret-link",
      }),
    ).rejects.toThrow("Cloudflare Email returned HTTP 403");
  });
});
