const games = ["Stack Overflow", "The Daily PR", "The Daily Fix"];

export function TerminalPreview() {
  return (
    <div className="terminal" aria-label="MVP terminal application preview">
      <div className="terminal-bar">
        <span>MVP</span>
        <span>alex / online</span>
      </div>
      <div className="terminal-body">
        <p className="terminal-kicker">MOST VALUED PROGRAMMER</p>
        <div className="terminal-score">
          <span>DAILY MVP</span>
          <strong>#2 / 9,180</strong>
        </div>
        <div className="terminal-games">
          {games.map((game, index) => (
            <p key={game} className={index === 0 ? "selected" : undefined}>
              <span aria-hidden="true">{index === 0 ? ">" : " "}</span> {game}
            </p>
          ))}
        </div>
        <p className="terminal-status">Codex / working</p>
      </div>
    </div>
  );
}
