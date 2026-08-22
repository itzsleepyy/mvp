import AsciiLogo from "@/components/ascii-logo";
import { InstallPill } from "@/components/install-pill";

export default function Home() {
  return (
    <main id="main-content">
      <section className="relative flex min-h-[calc(100vh-3.5rem)] flex-col items-center justify-center gap-12 overflow-hidden px-4">
        <h1 className="sr-only">MVP</h1>
        <AsciiLogo />
        <div className="relative z-10">
          <InstallPill />
        </div>
      </section>
    </main>
  );
}
