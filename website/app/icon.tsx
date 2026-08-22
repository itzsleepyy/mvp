import { ImageResponse } from "next/og";

export const size = { width: 64, height: 64 };
export const contentType = "image/png";

export default function Icon() {
  return new ImageResponse(
    <div
      style={{
        alignItems: "center",
        background: "#050505",
        color: "#a6ff6f",
        display: "flex",
        fontFamily: "monospace",
        fontSize: 28,
        height: "100%",
        justifyContent: "center",
        width: "100%",
      }}
    >
      MVP
    </div>,
    size,
  );
}
