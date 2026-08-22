"use client";

import { useState } from "react";
import { Check, Copy } from "lucide-react";
import { Button } from "@/components/ui/button";

export const INSTALL_COMMAND = "npm install mvp";

export function InstallPill() {
  const [status, setStatus] = useState<"idle" | "copied" | "failed">("idle");

  async function copyCommand() {
    try {
      if (!navigator.clipboard) throw new Error("Clipboard unavailable");
      await navigator.clipboard.writeText(INSTALL_COMMAND);
      setStatus("copied");
      window.setTimeout(() => setStatus("idle"), 1_500);
    } catch {
      setStatus("failed");
    }
  }

  return (
    <div
      id="install"
      className="inline-flex items-center gap-2 rounded-full border border-border bg-card py-1.5 pl-5 pr-1.5 shadow-sm"
    >
      <code className="text-sm text-foreground sm:text-base">
        <span aria-hidden="true" className="text-primary">
          ${" "}
        </span>
        {INSTALL_COMMAND}
      </code>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        onClick={copyCommand}
        aria-live="polite"
        className="rounded-full"
      >
        {status === "copied" ? <Check /> : <Copy />}
        {status === "copied" ? "Copied" : status === "failed" ? "Copy failed" : "Copy"}
      </Button>
    </div>
  );
}
