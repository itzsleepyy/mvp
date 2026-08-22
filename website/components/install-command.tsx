"use client";

import { useState } from "react";

const INSTALL_COMMAND =
  "git clone https://github.com/itzsleepyy/waitstate mvp && cd mvp && cargo install --path package";

export function InstallCommand() {
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
    <div className="install-command">
      <code>
        <span aria-hidden="true">$ </span>
        {INSTALL_COMMAND}
      </code>
      <button type="button" onClick={copyCommand} aria-live="polite">
        {status === "copied" ? "Copied" : status === "failed" ? "Copy failed" : "Copy"}
      </button>
    </div>
  );
}
