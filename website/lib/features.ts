export function emailAuthEnabled(): boolean {
  return process.env.EMAIL_AUTH_ENABLED === "true";
}
