import type { Metadata } from "next";
import { Pixelify_Sans, Space_Grotesk } from "next/font/google";
import { Footer } from "@/components/footer";
import { Navbar } from "@/components/navbar";
import "./globals.css";
import { cn } from "@/lib/utils";

const spaceGrotesk = Space_Grotesk({
  subsets: ["latin"],
  variable: "--font-sans",
  display: "swap",
});

const pixelify = Pixelify_Sans({
  weight: "400",
  subsets: ["latin"],
  display: "swap",
  fallback: ["MS PGothic", "monospace"],
  variable: "--font-pixelify",
});

const siteUrl = process.env.NEXT_PUBLIC_SITE_URL ?? "http://localhost:3001";

export const metadata: Metadata = {
  metadataBase: new URL(siteUrl),
  title: {
    default: "MVP - Most Valued Programmer",
    template: "%s / MVP",
  },
  description:
    "A competitive terminal arcade for programmers. Play while your coding agents work and compete to become the daily MVP.",
  alternates: { canonical: "/" },
  openGraph: {
    type: "website",
    url: "/",
    siteName: "MVP - Most Valued Programmer",
    title: "MVP - Most Valued Programmer",
    description:
      "A competitive terminal arcade for programmers. Play while your coding agents work and compete to become the daily MVP.",
    images: [{ url: "/opengraph-image", width: 1200, height: 630 }],
  },
  twitter: {
    card: "summary_large_image",
    title: "MVP - Most Valued Programmer",
    description: "The competitive terminal arcade for programmers.",
    images: ["/opengraph-image"],
  },
};

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html
      lang="en"
      className={cn(spaceGrotesk.variable, pixelify.variable, "font-sans")}
    >
      <body className="flex min-h-dvh flex-col">
        <a className="skip-link" href="#main-content">
          Skip to content
        </a>
        <Navbar />
        {children}
        <Footer />
      </body>
    </html>
  );
}
