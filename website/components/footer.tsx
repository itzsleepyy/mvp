const REPOSITORY = "https://github.com/itzsleepyy/waitstate";

export function Footer() {
  return (
    <footer className="footer shell">
      <p>MVP - Most Valued Programmer</p>
      <div>
        <a href={REPOSITORY}>GitHub</a>
        <a href={`${REPOSITORY}/blob/main/LICENSE`}>MIT License</a>
      </div>
    </footer>
  );
}
