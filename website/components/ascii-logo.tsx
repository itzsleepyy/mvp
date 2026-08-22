"use client";

// The MVP wordmark rendered as the same FIGlet "doh" ASCII art the package
// prints in the terminal menu (see package/src/ui.rs DOH_LOGO). Each cell is a
// fixed-width inline-block so the proportional Pixelify font still produces a
// perfect monospace grid, and a requestAnimationFrame loop animates a soft
// brightness wave across the glyphs.

import { useEffect, useRef } from "react";

const DOH_LOGO = [
  "MMMMMMMM               MMMMMMMMVVVVVVVV           VVVVVVVVPPPPPPPPPPPPPPPPP",
  "M:::::::M             M:::::::MV::::::V           V::::::VP::::::::::::::::P",
  "M::::::::M           M::::::::MV::::::V           V::::::VP::::::PPPPPP:::::P",
  "M:::::::::M         M:::::::::MV::::::V           V::::::VPP:::::P     P:::::P",
  "M::::::::::M       M::::::::::M V:::::V           V:::::V   P::::P     P:::::P",
  "M:::::::::::M     M:::::::::::M  V:::::V         V:::::V    P::::P     P:::::P",
  "M:::::::M::::M   M::::M:::::::M   V:::::V       V:::::V     P::::PPPPPP:::::P",
  "M::::::M M::::M M::::M M::::::M    V:::::V     V:::::V      P:::::::::::::PP",
  "M::::::M  M::::M::::M  M::::::M     V:::::V   V:::::V       P::::PPPPPPPPP",
  "M::::::M   M:::::::M   M::::::M      V:::::V V:::::V        P::::P",
  "M::::::M    M:::::M    M::::::M       V:::::V:::::V         P::::P",
  "M::::::M     MMMMM     M::::::M        V:::::::::V          P::::P",
  "M::::::M               M::::::M         V:::::::V         PP::::::PP",
  "M::::::M               M::::::M          V:::::V          P::::::::P",
  "M::::::M               M::::::M           V:::V           P::::::::P",
  "MMMMMMMM               MMMMMMMM            VVV            PPPPPPPPPP",
];

const COLS = 78;

export default function AsciiLogo() {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const root = ref.current;
    if (!root) return;
    const cells = root.querySelectorAll<HTMLSpanElement>("[data-cell]");

    if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) {
      cells.forEach((cell) => {
        cell.style.opacity = "1";
      });
      return;
    }

    let raf = 0;
    const start = performance.now();
    const tick = (now: number) => {
      const t = (now - start) / 1000;
      for (const cell of cells) {
        const x = Number(cell.dataset.x ?? 0);
        const y = Number(cell.dataset.y ?? 0);
        const wave = Math.sin(t * 2.4 - (x + y) * 0.18);
        cell.style.opacity = (0.35 + 0.65 * (0.5 + 0.5 * wave)).toFixed(3);
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  return (
    <div
      ref={ref}
      className="ascii-logo w-full text-center font-pixelify leading-none select-none"
      aria-hidden="true"
    >
      {DOH_LOGO.map((line, y) => (
        <div key={y} className="ascii-logo-row">
          {line.padEnd(COLS, " ").split("").map((ch, x) => {
            const hue = 105 + (x / (COLS - 1)) * 85;
            return (
              <span
                key={x}
                data-cell
                data-x={x}
                data-y={y}
                className="ascii-logo-cell"
                style={{ color: `hsl(${hue.toFixed(1)} 95% 68%)` }}
              >
                {ch === " " ? "\u00A0" : ch}
              </span>
            );
          })}
        </div>
      ))}
    </div>
  );
}
