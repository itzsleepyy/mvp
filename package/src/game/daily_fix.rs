//! The Daily Fix — spot the bug, fix it fastest. One short snippet per day,
//! identical for everyone (seeded from the day), with exactly one broken
//! line. Type the corrected line; the clock stops on the correct fix.
//! Wrong submissions cost time; after two misses a hint appears (and costs
//! more). Pure logic — no terminal types — so it can be unit tested and
//! simulated headlessly.

use std::time::Duration;

use super::GameInput;

/// Seconds added per wrong submission.
pub const WRONG_PENALTY_MS: u64 = 5_000;
/// Seconds added when the hint is revealed.
pub const HINT_PENALTY_MS: u64 = 15_000;
/// Wrong submissions before the hint appears automatically.
pub const HINT_AFTER_ATTEMPTS: u32 = 2;
/// Longest accepted fix line.
const FIX_LIMIT: usize = 200;

/// One curated bug. `lines[buggy_line]` is broken; the player must type
/// [`Bug::fix`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bug {
    /// Short name, e.g. "Off-by-one loop".
    pub title: &'static str,
    /// 1 (easy) to 3 (hard).
    pub difficulty: u8,
    /// Hint shown after two wrong submissions.
    pub hint: &'static str,
    /// One-line lesson shown after the fix.
    pub explainer: &'static str,
    /// The snippet, one string per line (including indentation).
    pub lines: &'static [&'static str],
    /// Index of the broken line in [`Bug::lines`].
    pub buggy_line: usize,
    /// The corrected line the player must type.
    pub fix: &'static str,
}

/// The curated bug bank, one entry per day (cycled).
pub const BANK: &[Bug] = &[
    Bug {
        title: "Off-by-one loop",
        difficulty: 1,
        hint: "The loop body runs one extra time",
        explainer: "0..=n is inclusive and runs n+1 times; 0..n is the fix.",
        lines: &[
            "fn count_items(items: Vec<i32>) -> i32 {",
            "    let mut count = 0;",
            "    for _ in 0..=items.len() {",
            "        count += 1;",
            "    }",
            "    count",
            "}",
        ],
        buggy_line: 2,
        fix: "    for _ in 0..items.len() {",
    },
    Bug {
        title: "Too young to drive",
        difficulty: 1,
        hint: "17-year-olds must be turned away",
        explainer: "age < 18 admits 17-year-olds; the driving age is 18.",
        lines: &["fn can_drive(age: u8) -> bool {", "    age < 18", "}"],
        buggy_line: 1,
        fix: "    age >= 18",
    },
    Bug {
        title: "Adding the wrong number",
        difficulty: 1,
        hint: "Only one of the inputs is used twice",
        explainer: "(a + a) / 2 ignores b; both inputs must be summed.",
        lines: &[
            "fn average(a: i32, b: i32) -> i32 {",
            "    (a + a) / 2",
            "}",
        ],
        buggy_line: 1,
        fix: "    (a + b) / 2",
    },
    Bug {
        title: "A bill that doesn't add up",
        difficulty: 2,
        hint: "Splitting 10 between 3 people loses a pound",
        explainer: "Integer division rounds down, so the total is never fully covered; ceil-divide instead.",
        lines: &[
            "fn split_bill(total: i32, people: i32) -> i32 {",
            "    total / people",
            "}",
        ],
        buggy_line: 1,
        fix: "    (total + people - 1) / people",
    },
    Bug {
        title: "The last element is skipped",
        difficulty: 1,
        hint: "The loop never touches the final element",
        explainer: "1..n stops before n; 1..=n includes it.",
        lines: &[
            "fn sum_to(n: i32) -> i32 {",
            "    let mut total = 0;",
            "    for i in 1..n {",
            "        total += i;",
            "    }",
            "    total",
            "}",
        ],
        buggy_line: 2,
        fix: "    for i in 1..=n {",
    },
    Bug {
        title: "Backwards range",
        difficulty: 2,
        hint: "The slice indices are in the wrong order",
        explainer: "text[end..start] is empty (or panics); slices run start..end.",
        lines: &[
            "fn substring(text: &str, start: usize, end: usize) -> &str {",
            "    &text[end..start]",
            "}",
        ],
        buggy_line: 1,
        fix: "    &text[start..end]",
    },
    Bug {
        title: "Even numbers test odd",
        difficulty: 1,
        hint: "Odd numbers leave a remainder of 1",
        explainer: "n % 2 == 1 is true for odd numbers — exactly backwards.",
        lines: &["fn is_even(n: i32) -> bool {", "    n % 2 == 1", "}"],
        buggy_line: 1,
        fix: "    n % 2 == 0",
    },
    Bug {
        title: "Inverted guard",
        difficulty: 1,
        hint: "The function name says what it must do",
        explainer: "age >= 18 flags adults; a minor is anyone under 18.",
        lines: &["fn is_minor(age: u8) -> bool {", "    age >= 18", "}"],
        buggy_line: 1,
        fix: "    age < 18",
    },
    Bug {
        title: "Count that never counts",
        difficulty: 1,
        hint: "The counter is never changed",
        explainer: "count; evaluates the value and throws it away; it must accumulate.",
        lines: &[
            "fn count_evens(xs: Vec<i32>) -> i32 {",
            "    let mut count = 0;",
            "    for x in xs {",
            "        if x % 2 == 0 {",
            "            count;",
            "        }",
            "    }",
            "    count",
            "}",
        ],
        buggy_line: 4,
        fix: "            count += 1;",
    },
    Bug {
        title: "Average ignores the size",
        difficulty: 2,
        hint: "The sum is returned without dividing",
        explainer: "The sum must be divided by the number of elements.",
        lines: &[
            "fn average(xs: &[f64]) -> f64 {",
            "    let mut total = 0.0;",
            "    for x in xs {",
            "        total += x;",
            "    }",
            "    return total;",
            "}",
        ],
        buggy_line: 5,
        fix: "    return total / xs.len() as f64;",
    },
    Bug {
        title: "Outputs not doubled",
        difficulty: 2,
        hint: "Every pushed value is the raw input",
        explainer: "out.push(x) copies the input; each value must be doubled.",
        lines: &[
            "fn doubled(xs: Vec<i32>) -> Vec<i32> {",
            "    let mut out = Vec::new();",
            "    for x in &xs {",
            "        out.push(x);",
            "    }",
            "    out",
            "}",
        ],
        buggy_line: 3,
        fix: "        out.push(x * 2);",
    },
    Bug {
        title: "Reading past the end",
        difficulty: 1,
        hint: "Indexes start at zero",
        explainer: "xs.len() is one past the last valid index; the last element is at len - 1.",
        lines: &["fn last(xs: &[i32]) -> i32 {", "    xs[xs.len()]", "}"],
        buggy_line: 1,
        fix: "    xs[xs.len() - 1]",
    },
    Bug {
        title: "Fail is not pass",
        difficulty: 1,
        hint: "Higher marks should pass",
        explainer: "mark <= 40 rewards failing marks; the threshold is 40 or more.",
        lines: &["fn passed(mark: i32) -> bool {", "    mark <= 40", "}"],
        buggy_line: 1,
        fix: "    mark >= 40",
    },
    Bug {
        title: "Swapped coordinates",
        difficulty: 1,
        hint: "The axes are written in the wrong order",
        explainer: "format!(\"({},{})\", y, x) prints y first; the arguments must be x, y.",
        lines: &[
            "fn format_point(x: i32, y: i32) -> String {",
            "    format!(\"({},{})\", y, x)",
            "}",
        ],
        buggy_line: 1,
        fix: "    format!(\"({},{})\", x, y)",
    },
    Bug {
        title: "Loop that never ends",
        difficulty: 2,
        hint: "n is never decremented",
        explainer: "n - 1; computes and discards; the loop counter must be assigned.",
        lines: &[
            "fn count_down(start: i32) -> i32 {",
            "    let mut n = start;",
            "    while n > 0 {",
            "        n - 1;",
            "    }",
            "    n",
            "}",
        ],
        buggy_line: 3,
        fix: "        n -= 1;",
    },
    Bug {
        title: "Float equality",
        difficulty: 3,
        hint: "Floating point values rarely land exactly",
        explainer: "0.5 is representable, but computed values drift; compare within a tolerance.",
        lines: &["fn is_half(v: f64) -> bool {", "    v == 0.5", "}"],
        buggy_line: 1,
        fix: "    (v - 0.5).abs() < 1e-9",
    },
    Bug {
        title: "Left edge rejected",
        difficulty: 2,
        hint: "The first column is out of bounds",
        explainer: "x > 0 rejects column 0; coordinates start at 0.",
        lines: &[
            "fn in_bounds(x: i32, width: i32) -> bool {",
            "    x > 0 && x < width",
            "}",
        ],
        buggy_line: 1,
        fix: "    x >= 0 && x < width",
    },
    Bug {
        title: "Seeded with an extra one",
        difficulty: 1,
        hint: "The running total starts at the wrong value",
        explainer: "Starting at 1 adds a phantom square; sums start at 0.",
        lines: &[
            "fn sum_squares(n: i32) -> i32 {",
            "    let mut total = 1;",
            "    for i in 1..=n {",
            "        total += i * i;",
            "    }",
            "    total",
            "}",
        ],
        buggy_line: 1,
        fix: "    let mut total = 0;",
    },
    Bug {
        title: "Missing factor",
        difficulty: 2,
        hint: "Multiplying by 1 is wasted and n is never included",
        explainer: "1..n starts with a useless x1 and stops before n; 2..=n is exactly right.",
        lines: &[
            "fn factorial(n: u64) -> u64 {",
            "    let mut result = 1;",
            "    for i in 1..n {",
            "        result *= i;",
            "    }",
            "    result",
            "}",
        ],
        buggy_line: 2,
        fix: "    for i in 2..=n {",
    },
    Bug {
        title: "A deposit that withdraws",
        difficulty: 1,
        hint: "Deposits add money",
        explainer: "balance - amount is a withdrawal; deposits must add.",
        lines: &[
            "fn deposit(balance: i32, amount: i32) -> i32 {",
            "    balance - amount",
            "}",
        ],
        buggy_line: 1,
        fix: "    balance + amount",
    },
    Bug {
        title: "Remainder instead of result",
        difficulty: 2,
        hint: "The whole part is wanted, not the remainder",
        explainer: "a % b is the remainder; a / b is the quotient.",
        lines: &["fn quotient(a: i32, b: i32) -> i32 {", "    a % b", "}"],
        buggy_line: 1,
        fix: "    a / b",
    },
    Bug {
        title: "Warning comes too late",
        difficulty: 1,
        hint: "The boundary value should already warn",
        explainer: "level > 10 lets level 10 pass silently; the threshold is >= 10.",
        lines: &[
            "fn warn(level: u8) -> &'static str {",
            "    if level > 10 { \"HIGH\" } else { \"low\" }",
            "}",
        ],
        buggy_line: 1,
        fix: "    if level >= 10 { \"HIGH\" } else { \"low\" }",
    },
    Bug {
        title: "Zero is not negative",
        difficulty: 1,
        hint: "The filter keeps values it should drop",
        explainer: "x > 0 lets zero through; negative numbers are strictly below zero.",
        lines: &[
            "fn negatives(xs: Vec<i32>) -> Vec<i32> {",
            "    let mut out = Vec::new();",
            "    for x in xs {",
            "        if x > 0 {",
            "            continue;",
            "        }",
            "        out.push(x);",
            "    }",
            "    out",
            "}",
        ],
        buggy_line: 3,
        fix: "        if x >= 0 {",
    },
    Bug {
        title: "Losing the first word",
        difficulty: 2,
        hint: "The first word is never written",
        explainer: "1..len skips the first word; 0..len covers them all.",
        lines: &[
            "fn initials(name: &str) -> String {",
            "    let mut out = String::new();",
            "    let words: Vec<&str> = name.split_whitespace().collect();",
            "    for i in 1..words.len() {",
            "        out.push(words[i].chars().next().unwrap());",
            "    }",
            "    out",
            "}",
        ],
        buggy_line: 3,
        fix: "    for i in 0..words.len() {",
    },
    Bug {
        title: "Loop bound off by two",
        difficulty: 2,
        hint: "One element is skipped at each end",
        explainer: "1..len-1 skips both the first and the last element.",
        lines: &[
            "fn join(xs: Vec<String>) -> String {",
            "    let mut out = String::new();",
            "    for i in 1..xs.len() - 1 {",
            "        out.push_str(&xs[i]);",
            "    }",
            "    out",
            "}",
        ],
        buggy_line: 2,
        fix: "    for i in 0..xs.len() {",
    },
];

/// A run of The Daily Fix: today's bug, an input buffer and a clock.
pub struct DailyFix {
    bug: &'static Bug,
    input: String,
    attempts: u32,
    hint_shown: bool,
    penalty_ms: u64,
    solved: bool,
    elapsed_millis: u64,
}

impl DailyFix {
    pub fn new(seed: u64) -> Self {
        Self {
            bug: &BANK[seed as usize % BANK.len()],
            input: String::new(),
            attempts: 0,
            hint_shown: false,
            penalty_ms: 0,
            solved: false,
            elapsed_millis: 0,
        }
    }

    /// Advances the clock. Turn-based, so only the timer moves; real
    /// elapsed time counts even after a long stall.
    pub fn update(&mut self, dt: Duration) {
        if self.solved {
            return;
        }
        self.elapsed_millis += dt.as_millis() as u64;
    }

    pub fn handle_input(&mut self, input: GameInput) {
        match input {
            GameInput::Type(c) => self.type_char(c),
            GameInput::Backspace => {
                if !self.solved {
                    self.input.pop();
                }
            }
            GameInput::Confirm => self.submit(),
            GameInput::Jump => {}
        }
    }

    fn type_char(&mut self, c: char) {
        if self.solved || c.is_control() {
            return;
        }
        if self.input.chars().count() >= FIX_LIMIT {
            return;
        }
        self.input.push(c);
    }

    /// Submits the typed line. A wrong fix costs time, clears the input
    /// and counts an attempt; after two misses the hint appears and costs
    /// more.
    fn submit(&mut self) {
        if self.solved {
            return;
        }
        if normalize(&self.input) == normalize(self.bug.fix) {
            self.solved = true;
            return;
        }
        self.attempts += 1;
        self.penalty_ms += WRONG_PENALTY_MS;
        self.input.clear();
        if self.attempts >= HINT_AFTER_ATTEMPTS && !self.hint_shown {
            self.hint_shown = true;
            self.penalty_ms += HINT_PENALTY_MS;
        }
    }

    // ---- accessors ---------------------------------------------------------

    pub fn is_game_over(&self) -> bool {
        self.solved
    }

    /// The run score: `1_000_000_000 - milliseconds` — the fastest correct
    /// fix wins. Zero until solved.
    pub fn score(&self) -> u64 {
        if !self.solved {
            return 0;
        }
        1_000_000_000u64
            .saturating_sub(self.elapsed_millis + self.penalty_ms)
            .max(1)
    }

    #[cfg(test)]
    pub fn elapsed(&self) -> f64 {
        self.elapsed_millis as f64 / 1000.0
    }

    /// Total time charged, including penalties from wrong fixes and the
    /// hint.
    pub fn total_seconds(&self) -> f64 {
        (self.elapsed_millis + self.penalty_ms) as f64 / 1000.0
    }

    pub fn bug(&self) -> &'static Bug {
        self.bug
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    pub fn hint_shown(&self) -> bool {
        self.hint_shown
    }
}

/// Collapses whitespace runs so a fix typed with loose spacing still
/// matches.
pub fn normalize(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn type_line(game: &mut DailyFix, line: &str) {
        for c in line.chars() {
            game.handle_input(GameInput::Type(c));
        }
    }

    #[test]
    fn the_daily_bug_is_seeded_and_stable() {
        let a = DailyFix::new(7);
        let b = DailyFix::new(7);
        assert_eq!(a.bug(), b.bug());
        assert_eq!(a.bug().title, BANK[7 % BANK.len()].title);
        assert!(!a.is_game_over());
        assert_eq!(a.score(), 0);
    }

    #[test]
    fn typing_the_fix_solves_the_run() {
        let mut game = DailyFix::new(0);
        let fix = game.bug().fix;
        type_line(&mut game, fix);
        game.handle_input(GameInput::Confirm);
        assert!(game.is_game_over());
        assert!(game.score() > 0);
        assert_eq!(game.attempts(), 0);
    }

    #[test]
    fn whitespace_is_normalized_for_matching() {
        let mut game = DailyFix::new(0);
        let fix = game.bug().fix;
        // Loose spacing around the operators still matches.
        type_line(
            &mut game,
            &fix.split_whitespace().collect::<Vec<_>>().join("  "),
        );
        game.handle_input(GameInput::Confirm);
        assert!(game.is_game_over(), "re-spaced fix must match");
    }

    #[test]
    fn wrong_fixes_cost_time_and_eventually_reveal_the_hint() {
        let mut game = DailyFix::new(0);
        type_line(&mut game, "    for _ in 99..items.len() {");
        game.handle_input(GameInput::Confirm);
        assert!(!game.is_game_over());
        assert_eq!(game.attempts(), 1);
        assert_eq!(game.total_seconds(), 5.0);
        assert!(!game.hint_shown());

        type_line(&mut game, "    still wrong");
        game.handle_input(GameInput::Confirm);
        assert_eq!(game.attempts(), 2);
        assert!(game.hint_shown(), "hint appears after two misses");
        assert_eq!(game.total_seconds(), 5.0 + 5.0 + 15.0);

        let mut slow = DailyFix::new(0);
        slow.update(Duration::from_secs(1));
        type_line(&mut slow, "    for _ in 99..items.len() {");
        slow.handle_input(GameInput::Confirm);
        assert_eq!(slow.total_seconds(), 1.0 + 5.0, "elapsed plus penalty");
    }

    #[test]
    fn scoring_prefers_speed() {
        let fast = {
            let mut game = DailyFix::new(3);
            let fix = game.bug().fix;
            type_line(&mut game, fix);
            game.handle_input(GameInput::Confirm);
            game
        };
        let slow = {
            let mut game = DailyFix::new(3);
            game.update(Duration::from_secs(90));
            let fix = game.bug().fix;
            type_line(&mut game, fix);
            game.handle_input(GameInput::Confirm);
            game
        };
        assert!(fast.score() > slow.score());
        assert_eq!(fast.bug(), slow.bug(), "same seed, same bug");
    }

    #[test]
    fn penalties_reduce_the_score() {
        let clean = {
            let mut game = DailyFix::new(3);
            let fix = game.bug().fix;
            type_line(&mut game, fix);
            game.handle_input(GameInput::Confirm);
            game.score()
        };
        let sloppy = {
            let mut game = DailyFix::new(3);
            type_line(&mut game, "nope");
            game.handle_input(GameInput::Confirm);
            let fix = game.bug().fix;
            type_line(&mut game, fix);
            game.handle_input(GameInput::Confirm);
            game.score()
        };
        assert!(clean > sloppy);
    }

    #[test]
    fn backspace_edits_and_jump_is_ignored() {
        let mut game = DailyFix::new(0);
        let fix = game.bug().fix;
        type_line(&mut game, fix);
        game.handle_input(GameInput::Backspace);
        game.handle_input(GameInput::Backspace);
        assert_eq!(game.input().len(), fix.len() - 2, "backspace edits");

        game.handle_input(GameInput::Jump);
        assert!(!game.is_game_over());

        // Clearing the buffer and typing the fix solves it.
        for _ in 0..game.input().len() {
            game.handle_input(GameInput::Backspace);
        }
        assert!(game.input().is_empty());
        type_line(&mut game, fix);
        game.handle_input(GameInput::Confirm);
        assert!(game.is_game_over());
    }

    #[test]
    fn solved_runs_ignore_further_input() {
        let mut game = DailyFix::new(0);
        let fix = game.bug().fix;
        type_line(&mut game, fix);
        game.handle_input(GameInput::Confirm);
        let score = game.score();
        game.handle_input(GameInput::Type('x'));
        game.handle_input(GameInput::Confirm);
        game.update(Duration::from_secs(5));
        assert_eq!(game.score(), score);
        assert_eq!(game.attempts(), 0);
    }

    #[test]
    fn the_bank_is_integrity_checked() {
        assert!(BANK.len() >= 20, "bank size: {}", BANK.len());
        for (i, bug) in BANK.iter().enumerate() {
            assert!(!bug.title.is_empty(), "bug {i}");
            assert!((1..=3).contains(&bug.difficulty), "bug {i} difficulty");
            assert!(!bug.hint.is_empty(), "bug {i} hint");
            assert!(!bug.explainer.is_empty(), "bug {i} explainer");
            assert!(bug.lines.len() >= 3, "bug {i} lines");
            assert!(bug.buggy_line < bug.lines.len(), "bug {i} buggy index");
            assert!(!bug.fix.is_empty(), "bug {i} fix");
            assert_ne!(
                normalize(bug.fix),
                normalize(bug.lines[bug.buggy_line]),
                "bug {i}: the fix must differ from the broken line"
            );
            // The broken line must not silently compile-differ in a way
            // that makes the whole snippet wrong: sanity only.
            for line in bug.lines {
                assert!(line.len() <= FIX_LIMIT, "bug {i}: line too long: {line}");
            }
        }
    }
}
