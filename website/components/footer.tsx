const REPOSITORY = "https://github.com/itzsleepyy/waitstate";

export function Footer() {
  return (
    <footer className="border-t border-border">
      <div className="mx-auto flex h-14 w-full max-w-5xl items-center justify-between px-4 text-sm text-muted-foreground sm:px-6">
        <p>MVP - Most Valued Programmer</p>
        <div className="flex gap-6">
          <a href={REPOSITORY} className="transition-colors hover:text-foreground">
            GitHub
          </a>
          <a
            href={`${REPOSITORY}/blob/main/LICENSE`}
            className="transition-colors hover:text-foreground"
          >
            MIT License
          </a>
        </div>
      </div>
    </footer>
  );
}
