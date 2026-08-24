import Link from "next/link";
import { NavbarBackdrop } from "@/components/navbar-backdrop";
import { NavLinks } from "@/components/nav-links";
import { StatusPill } from "@/components/status-pill";

export function Navbar() {
  return (
    <header className="sticky top-0 z-50 h-14">
      <NavbarBackdrop />
      <div className="mx-auto grid h-full w-full max-w-5xl grid-cols-[1fr_auto_1fr] items-center px-4 pt-3 sm:px-6">
        <Link
          href="/"
          className="justify-self-start font-pixelify text-2xl leading-none tracking-wide text-foreground sm:text-3xl"
          aria-label="MVP home"
        >
          MVP
        </Link>
        <NavLinks />
        <div className="justify-self-end">
          <StatusPill />
        </div>
      </div>
    </header>
  );
}
