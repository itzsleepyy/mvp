export interface EmailMessage {
  from: string;
  to: string;
  subject: string;
  text: string;
  html: string;
}

export interface EmailProvider {
  send(message: EmailMessage): Promise<void>;
}

export function cloudflareEmailProvider(
  accountId: string,
  apiToken: string,
): EmailProvider {
  return {
    async send(message) {
      const response = await fetch(
        `https://api.cloudflare.com/client/v4/accounts/${encodeURIComponent(accountId)}/email/sending/send`,
        {
          method: "POST",
          headers: {
            Authorization: `Bearer ${apiToken}`,
            "Content-Type": "application/json",
          },
          body: JSON.stringify({
            from: message.from,
            to: message.to,
            subject: message.subject,
            text: message.text,
            html: message.html,
          }),
          signal: AbortSignal.timeout(10_000),
        },
      );
      const result = (await response.json().catch(() => null)) as {
        success?: boolean;
      } | null;
      if (!response.ok || result?.success === false)
        throw new Error(
          `Cloudflare Email returned HTTP ${String(response.status)}`,
        );
    },
  };
}
