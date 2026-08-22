const BROWSER_TOKEN = /^[A-Za-z0-9._~-]{16,512}$/;

export function validBrowserToken(value: unknown): value is string {
  return typeof value === "string" && BROWSER_TOKEN.test(value);
}
