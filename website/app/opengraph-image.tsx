import { ImageResponse } from "next/og";

export const alt = "MVP - Most Valued Programmer";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

export default function OpenGraphImage() {
  return new ImageResponse(
    <div
      style={{
        alignItems: "flex-start",
        background: "#050505",
        color: "#f2f0e9",
        display: "flex",
        flexDirection: "column",
        fontFamily: "monospace",
        height: "100%",
        justifyContent: "space-between",
        padding: "72px",
        width: "100%",
      }}
    >
      <div style={{ color: "#a6ff6f", display: "flex", fontSize: 30 }}>
        MVP / MOST VALUED PROGRAMMER
      </div>
      <div
        style={{ display: "flex", fontSize: 180, lineHeight: 1, maxWidth: 1000 }}
      >
        MVP
      </div>
      <div style={{ color: "#8c8c87", display: "flex", fontSize: 24 }}>
        npx @mvp-play/cli
      </div>
    </div>,
    size,
  );
}
