//! The Daily PR — Wordle for developers. One 5-letter word per day,
//! identical for everyone (seeded from the day), six guesses, feedback on
//! every letter. Fewest guesses wins; ties break on speed. Pure logic — no
//! terminal types — so it can be unit tested and simulated headlessly.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::GameInput;
pub use super::words::{GUESSES, WORDS};

/// Guesses allowed per puzzle.
pub const MAX_GUESSES: usize = 6;
/// The length of the daily word.
pub const WORD_LEN: usize = 5;
/// Score headroom per unused guess: a solve in N guesses scores
/// `(7 - N) * GUESS_WEIGHT - seconds`, so fewer guesses always beats more
/// and a faster solve breaks the tie.
pub const GUESS_WEIGHT: u64 = 1_000_000;

/// Persisted progress for today's board. The answer is derived from `day`,
/// so only submitted guesses and elapsed time need to be stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyPrProgress {
    pub day: u64,
    pub guesses: Vec<String>,
    pub elapsed_millis: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_run_id: Option<uuid::Uuid>,
}

/// Feedback for one letter of a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// Correct letter, correct position.
    Correct,
    /// Correct letter, wrong position.
    Present,
    /// Letter not in the word.
    Absent,
}

/// One evaluated guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guess {
    pub word: String,
    pub marks: [Mark; WORD_LEN],
}

/// The state of a daily-word run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrState {
    /// Guessing; the board is open.
    Playing,
    /// The word was guessed; the board locks.
    Solved,
    /// All six guesses were used; the word is revealed.
    Failed,
}

/// A run of The Daily PR: one word, six guesses, a locked board.
pub struct DailyPr {
    /// The daily word, uppercase.
    word: [char; WORD_LEN],
    /// The day ordinal — displayed as the "commit number" of the PR.
    commit_number: u64,
    guesses: Vec<Guess>,
    /// The letters typed for the current, not-yet-submitted guess.
    input: String,
    /// Transient status line ("NOT A WORD", "TYPE 5 LETTERS").
    status: Option<&'static str>,
    state: PrState,
    elapsed_millis: u64,
    completion_run_id: Option<uuid::Uuid>,
}

impl DailyPr {
    pub fn new(seed: u64) -> Self {
        let word = WORDS[seed as usize % WORDS.len()];
        let word = word.to_ascii_uppercase();
        Self {
            word: word
                .chars()
                .collect::<Vec<_>>()
                .try_into()
                .expect("words are exactly 5 letters"),
            commit_number: seed,
            guesses: Vec::new(),
            input: String::new(),
            status: None,
            state: PrState::Playing,
            elapsed_millis: 0,
            completion_run_id: None,
        }
    }

    /// Rebuilds today's board from persisted submitted guesses. A stale
    /// progress record is ignored when the day changes.
    pub fn restore(seed: u64, progress: Option<DailyPrProgress>) -> Self {
        let mut game = Self::new(seed);
        let Some(progress) = progress.filter(|saved| saved.day == seed) else {
            return game;
        };
        let completion_run_id = progress.completion_run_id;
        for guess in progress.guesses.into_iter().take(MAX_GUESSES) {
            if game.state != PrState::Playing {
                break;
            }
            game.input = guess;
            game.submit();
        }
        game.elapsed_millis = progress.elapsed_millis;
        if completion_run_id.is_some() {
            game.completion_run_id = completion_run_id;
        }
        game.input.clear();
        game.status = None;
        game
    }

    /// Captures the durable parts of the board. Partially typed input is
    /// intentionally omitted; it has not consumed an attempt yet.
    pub fn progress(&self) -> DailyPrProgress {
        DailyPrProgress {
            day: self.commit_number,
            guesses: self
                .guesses
                .iter()
                .map(|guess| guess.word.clone())
                .collect(),
            elapsed_millis: self.elapsed_millis,
            completion_run_id: self.completion_run_id,
        }
    }

    /// Advances the run clock. The puzzle is turn-based, so only the timer
    /// moves; real elapsed time counts even after a long stall.
    pub fn update(&mut self, dt: Duration) {
        if self.state != PrState::Playing {
            return;
        }
        self.elapsed_millis += dt.as_millis() as u64;
    }

    pub fn handle_input(&mut self, input: GameInput) {
        match input {
            GameInput::Type(c) => self.type_letter(c),
            GameInput::Backspace => self.backspace(),
            GameInput::Confirm => self.submit(),
            GameInput::Jump => {}
        }
    }

    fn type_letter(&mut self, c: char) {
        if self.state != PrState::Playing || !c.is_ascii_alphabetic() {
            return;
        }
        if self.input.chars().count() >= WORD_LEN {
            return;
        }
        self.input.push(c.to_ascii_uppercase());
        self.status = None;
    }

    fn backspace(&mut self) {
        if self.state != PrState::Playing {
            return;
        }
        self.input.pop();
        self.status = None;
    }

    /// Submits the current input as a guess. Rejects anything that is not a
    /// five-letter word from the dictionary.
    fn submit(&mut self) {
        if self.state != PrState::Playing {
            return;
        }
        if self.input.chars().count() != WORD_LEN {
            self.status = Some("TYPE 5 LETTERS");
            return;
        }
        if !GUESSES.contains(&self.input.to_ascii_lowercase().as_str()) {
            self.status = Some("NOT A WORD");
            return;
        }
        let marks = evaluate(&self.input, &self.word);
        let solved = marks.iter().all(|m| *m == Mark::Correct);
        self.guesses.push(Guess {
            word: self.input.clone(),
            marks,
        });
        self.input.clear();
        self.status = None;
        self.state = if solved {
            PrState::Solved
        } else if self.guesses.len() >= MAX_GUESSES {
            PrState::Failed
        } else {
            PrState::Playing
        };
        if self.state != PrState::Playing && self.completion_run_id.is_none() {
            self.completion_run_id = Some(uuid::Uuid::new_v4());
        }
    }

    // ---- accessors ---------------------------------------------------------

    pub fn is_game_over(&self) -> bool {
        self.state != PrState::Playing
    }

    /// The run score: `(7 - guesses) * GUESS_WEIGHT - seconds`. Fewer
    /// guesses always beats more; a faster solve breaks the tie. A failed
    /// puzzle scores zero (a did-not-finish, never a record).
    pub fn score(&self) -> u64 {
        match self.state {
            PrState::Solved => {
                let unused = (MAX_GUESSES as u64 + 1) - self.guesses.len() as u64;
                (unused * GUESS_WEIGHT).saturating_sub(self.elapsed_millis / 1000)
            }
            PrState::Playing | PrState::Failed => 0,
        }
    }

    pub fn elapsed(&self) -> f64 {
        self.elapsed_millis as f64 / 1000.0
    }

    pub fn elapsed_millis(&self) -> u64 {
        self.elapsed_millis
    }

    /// The number of guesses used (0 while playing).
    pub fn guesses(&self) -> usize {
        self.guesses.len()
    }

    pub fn state(&self) -> PrState {
        self.state
    }

    pub fn word(&self) -> String {
        self.word.iter().collect()
    }

    /// The daily PR's "commit number" (the day ordinal).
    pub fn commit_number(&self) -> u64 {
        self.commit_number
    }

    pub fn completion_run_id(&self) -> Option<uuid::Uuid> {
        self.completion_run_id
    }

    pub fn guesses_used(&self) -> &[Guess] {
        &self.guesses
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn status(&self) -> Option<&'static str> {
        self.status
    }

    /// Whether the word was solved and the board locked (or the run
    /// failed): further input does nothing.
    pub fn locked(&self) -> bool {
        self.state != PrState::Playing
    }
}

/// Scores a guess against the word: correct positions first, then the
/// remaining letters in the word, so each letter is marked at most once.
pub fn evaluate(guess: &str, word: &[char; WORD_LEN]) -> [Mark; WORD_LEN] {
    let chars: Vec<char> = guess.chars().collect();
    let mut used = [false; WORD_LEN];
    let mut marks = [Mark::Absent; WORD_LEN];
    for i in 0..WORD_LEN {
        if chars[i] == word[i] {
            marks[i] = Mark::Correct;
            used[i] = true;
        }
    }
    for i in 0..WORD_LEN {
        if marks[i] == Mark::Correct {
            continue;
        }
        for j in 0..WORD_LEN {
            if !used[j] && chars[i] == word[j] {
                marks[i] = Mark::Present;
                used[j] = true;
                break;
            }
        }
    }
    marks
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn word_of(seed: u64) -> String {
        WORDS[seed as usize % WORDS.len()].to_ascii_uppercase()
    }

    fn solve(game: &mut DailyPr, word: &str) {
        for c in word.chars() {
            game.handle_input(GameInput::Type(c));
        }
        game.handle_input(GameInput::Confirm);
    }

    #[test]
    fn the_daily_word_is_seeded_and_stable() {
        let a = DailyPr::new(7);
        let b = DailyPr::new(7);
        assert_eq!(a.word(), b.word());
        assert_eq!(a.word(), word_of(7));
        assert_eq!(a.commit_number(), 7);
        assert_eq!(a.word().len(), WORD_LEN);
    }

    #[test]
    fn persisted_guesses_restore_and_stale_days_do_not() {
        let seed = 7;
        let mut game = DailyPr::new(seed);
        let word = game.word();
        let filler = GUESSES
            .iter()
            .find(|guess| !guess.eq_ignore_ascii_case(&word))
            .unwrap();
        solve(&mut game, filler);
        game.update(Duration::from_secs(12));

        let restored = DailyPr::restore(seed, Some(game.progress()));
        assert_eq!(restored.guesses(), 1);
        assert_eq!(restored.elapsed(), 12.0);
        assert_eq!(restored.guesses_used()[0].word, filler.to_ascii_uppercase());

        let tomorrow = DailyPr::restore(seed + 1, Some(game.progress()));
        assert_eq!(tomorrow.guesses(), 0);
        assert_eq!(tomorrow.elapsed(), 0.0);
    }

    #[test]
    fn typing_uppercases_and_caps_at_five_letters() {
        let mut game = DailyPr::new(1);
        for c in ['a', 'b', 'c', 'd', 'e', 'f', '3', '!'] {
            game.handle_input(GameInput::Type(c));
        }
        assert_eq!(game.input(), "ABCDE", "digits and symbols are ignored");
        game.handle_input(GameInput::Backspace);
        assert_eq!(game.input(), "ABCD");
    }

    #[test]
    fn wrong_length_and_unknown_words_are_rejected() {
        let mut game = DailyPr::new(1);
        game.handle_input(GameInput::Type('a'));
        game.handle_input(GameInput::Confirm);
        assert_eq!(game.status(), Some("TYPE 5 LETTERS"));
        assert_eq!(game.guesses(), 0);
        assert_eq!(game.state(), PrState::Playing);

        let mut game = DailyPr::new(1);
        solve(&mut game, "ZZZZZ");
        assert_eq!(game.status(), Some("NOT A WORD"));
        assert_eq!(game.guesses(), 0);
    }

    #[test]
    fn solving_marks_every_letter_and_locks_the_board() {
        let seed = 5;
        let word = word_of(seed);
        let mut game = DailyPr::new(seed);
        solve(&mut game, &word);
        assert_eq!(game.state(), PrState::Solved);
        assert_eq!(game.guesses(), 1);
        assert!(
            game.guesses_used()[0]
                .marks
                .iter()
                .all(|m| *m == Mark::Correct)
        );
        assert!(game.score() > 0);
        // The board is locked: further input does nothing.
        let score = game.score();
        solve(&mut game, &word_of(seed + 1));
        assert_eq!(game.guesses(), 1);
        assert_eq!(game.score(), score);
    }

    #[test]
    fn six_wrong_guesses_fail_the_run() {
        let seed = 5;
        let word = word_of(seed);
        let mut game = DailyPr::new(seed);
        let filler = GUESSES
            .iter()
            .find(|w| **w != word.to_ascii_lowercase())
            .expect("a word different from the answer exists");
        for _ in 0..MAX_GUESSES {
            solve(&mut game, filler);
        }
        assert_eq!(game.state(), PrState::Failed);
        assert_eq!(game.score(), 0);
        assert_eq!(game.guesses(), MAX_GUESSES);
    }

    #[test]
    fn fewer_guesses_beat_more_and_faster_breaks_ties() {
        let seed = 42;
        let word = word_of(seed);
        let filler = GUESSES
            .iter()
            .find(|w| **w != word)
            .expect("a word different from the answer exists");

        let mut slow = DailyPr::new(seed);
        slow.update(Duration::from_secs(60)); // timer runs while guessing
        solve(&mut slow, &word);
        let slow_score = slow.score();

        let mut fast = DailyPr::new(seed);
        solve(&mut fast, &word);
        let fast_score = fast.score();

        assert!(fast_score > slow_score, "same guesses, faster wins");

        let mut many = DailyPr::new(seed);
        for _ in 0..2 {
            solve(&mut many, filler);
        }
        solve(&mut many, &word);
        many.update(Duration::from_secs(60));
        assert!(
            fast_score > many.score(),
            "1 guess must beat 3 guesses at equal speed"
        );
    }

    #[test]
    fn timer_only_runs_while_guessing() {
        let seed = 9;
        let word = word_of(seed);
        let mut game = DailyPr::new(seed);
        game.update(Duration::from_millis(500));
        assert!((game.elapsed() - 0.5).abs() < 0.05);
        solve(&mut game, &word);
        let frozen = game.elapsed();
        game.update(Duration::from_secs(1));
        assert_eq!(game.elapsed(), frozen);
    }

    #[test]
    fn jump_is_ignored() {
        let mut game = DailyPr::new(1);
        game.handle_input(GameInput::Jump);
        assert_eq!(game.state(), PrState::Playing);
        assert_eq!(game.guesses(), 0);
    }

    #[test]
    fn evaluate_marks_correct_present_and_absent() {
        let crane = ['C', 'R', 'A', 'N', 'E'];
        assert_eq!(evaluate("CRANE", &crane), [Mark::Correct; WORD_LEN]);

        assert_eq!(
            evaluate("ARISE", &crane),
            [
                Mark::Present, // A in word, wrong position
                Mark::Correct, // R in position 1
                Mark::Absent,  // I
                Mark::Absent,  // S
                Mark::Correct, // E in position 4
            ]
        );
    }

    #[test]
    fn evaluate_counts_each_letter_at_most_once() {
        // LEVEL has one V and two Es: the second guess letter can only be
        // Present once the word's E slots are exhausted.
        let level = ['L', 'E', 'V', 'E', 'L'];
        assert_eq!(
            evaluate("LEAVE", &level),
            [
                Mark::Correct, // L at 0
                Mark::Correct, // E at 1
                Mark::Absent,  // A
                Mark::Present, // V at 2 (wrong position)
                Mark::Present, // E at 4 (the word's second E)
            ]
        );

        // A correct letter consumes its word slot, so a second same-letter
        // guess cannot also be marked Present.
        let speed = ['S', 'P', 'E', 'E', 'D'];
        assert_eq!(
            evaluate("SEEKS", &speed),
            [
                Mark::Correct, // S at 0
                Mark::Present, // E at 1 (the word's first E)
                Mark::Correct, // E at 2
                Mark::Absent,  // K
                Mark::Absent,  // S: no second S left
            ]
        );
    }

    #[test]
    fn word_lists_are_sane() {
        assert!(WORDS.len() >= 500, "curated answers: {}", WORDS.len());
        assert!(GUESSES.len() >= 5000, "dictionary: {}", GUESSES.len());
        for w in WORDS {
            assert_eq!(w.len(), WORD_LEN, "{w}");
            assert!(
                w.chars().all(|c| c.is_ascii_alphabetic()),
                "{w} must be plain ASCII"
            );
            assert!(GUESSES.contains(w), "{w} must be guessable");
        }
        for w in ["ALERT", "CRANE", "LEMON", "TABLE", "STACK", "DEBUG"] {
            assert!(
                GUESSES.contains(&w.to_lowercase().as_str()),
                "{w} must be guessable"
            );
        }
    }
}
