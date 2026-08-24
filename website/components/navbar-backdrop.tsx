"use client";

import { useEffect, useState } from "react";

export function NavbarBackdrop() {
  const [scrolled, setScrolled] = useState(false);

  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 8);
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  return (
    <div
      aria-hidden="true"
      data-visible={scrolled}
      className="pointer-events-none absolute inset-x-0 inset-y-0 -z-10 bg-background/60 opacity-0 backdrop-blur-md transition-opacity duration-300 [mask-image:linear-gradient(to_bottom,black_75%,transparent)] data-[visible=true]:opacity-100"
    />
  );
}
