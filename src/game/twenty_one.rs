//! Twenty One — blackjack against the dealer. Turn-based, pure logic, no
//! terminal types, so it can be unit tested and simulated headlessly.

use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;

use super::GameInput;

/// Starting chip stack for a fresh run.
pub const STARTING_CHIPS: u64 = 1_000;
/// The fixed bet per round. A run ends when the bet can no longer be paid.
pub const BET: u64 = 100;
/// The dealer draws while below this total (stands on all 17s, soft or hard).
const DEALER_STAND: u8 = 17;
/// The deck is reshuffled when fewer cards than this remain, so the shoe
/// never runs dry mid-round.
const RESHUFFLE_AT: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suit {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
}

impl Suit {
    pub const ALL: [Suit; 4] = [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades];

    pub fn symbol(self) -> char {
        match self {
            Suit::Clubs => '♣',
            Suit::Diamonds => '♦',
            Suit::Hearts => '♥',
            Suit::Spades => '♠',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rank {
    Ace,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Jack,
    Queen,
    King,
}

impl Rank {
    pub const ALL: [Rank; 13] = [
        Rank::Ace,
        Rank::Two,
        Rank::Three,
        Rank::Four,
        Rank::Five,
        Rank::Six,
        Rank::Seven,
        Rank::Eight,
        Rank::Nine,
        Rank::Ten,
        Rank::Jack,
        Rank::Queen,
        Rank::King,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Rank::Ace => "A",
            Rank::Two => "2",
            Rank::Three => "3",
            Rank::Four => "4",
            Rank::Five => "5",
            Rank::Six => "6",
            Rank::Seven => "7",
            Rank::Eight => "8",
            Rank::Nine => "9",
            Rank::Ten => "10",
            Rank::Jack => "J",
            Rank::Queen => "Q",
            Rank::King => "K",
        }
    }

    /// The card's base value. Aces count as 11 here; [`hand_value`] downgrades
    /// them to 1 when needed to avoid a bust.
    pub fn value(self) -> u8 {
        match self {
            Rank::Ace => 11,
            Rank::Jack | Rank::Queen | Rank::King => 10,
            other => other as u8 + 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Card {
    pub rank: Rank,
    pub suit: Suit,
}

impl Card {
    fn new(rank: Rank, suit: Suit) -> Self {
        Self { rank, suit }
    }
}

/// Where a hand currently stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Waiting for the player to hit or stand.
    PlayerTurn,
    /// The hand is settled; ENTER deals the next round.
    RoundOver,
}

/// How the last round ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Win,
    Blackjack,
    Lose,
    Push,
}

/// Best value of the hand under standard ace rules: aces count as 11 while
/// possible, downgrading to 1 to avoid a bust. A bust is a value above 21.
pub fn hand_value(cards: &[Card]) -> u8 {
    let mut total = 0;
    let mut aces = 0;
    for card in cards {
        if card.rank == Rank::Ace {
            aces += 1;
        }
        total += card.rank.value();
    }
    while total > 21 && aces > 0 {
        total -= 10;
        aces -= 1;
    }
    total
}

/// A natural: exactly two cards totalling 21.
pub fn is_blackjack(cards: &[Card]) -> bool {
    cards.len() == 2 && hand_value(cards) == 21
}

/// The chip movement for each round outcome.
pub fn payout(outcome: Outcome) -> i64 {
    match outcome {
        Outcome::Win => BET as i64,
        Outcome::Blackjack => (BET + BET / 2) as i64,
        Outcome::Lose => -(BET as i64),
        Outcome::Push => 0,
    }
}

/// One blackjack run: a chip stack, a shoe and a dealer that never changes
/// strategy. Rounds are dealt, settled and scored; ENTER starts the next.
pub struct TwentyOne {
    rng: StdRng,
    deck: Vec<Card>,
    player: Vec<Card>,
    dealer: Vec<Card>,
    chips: u64,
    /// The highest chip stack this run ever reached. Fluctuating stacks make
    /// this the meaningful run score for the high-score board.
    peak_chips: u64,
    phase: Phase,
    outcome: Option<Outcome>,
    wins: u32,
    losses: u32,
    pushes: u32,
    game_over: bool,
}

impl TwentyOne {
    /// Creates a run with a seeded RNG for reproducible tests.
    pub fn new(seed: u64) -> Self {
        let mut game = Self {
            rng: StdRng::seed_from_u64(seed),
            deck: Vec::new(),
            player: Vec::new(),
            dealer: Vec::new(),
            chips: STARTING_CHIPS,
            peak_chips: STARTING_CHIPS,
            phase: Phase::PlayerTurn,
            outcome: None,
            wins: 0,
            losses: 0,
            pushes: 0,
            game_over: false,
        };
        game.start_round();
        game
    }

    /// Advances the simulation. Twenty One is turn-based, so this is a no-op:
    /// the game state only moves on player input.
    pub fn update(&mut self, _dt: std::time::Duration) {}

    pub fn handle_input(&mut self, input: GameInput) {
        match input {
            GameInput::Hit => self.hit(),
            GameInput::Stand => self.stand(),
            GameInput::Confirm => self.confirm(),
            GameInput::Jump => {}
        }
    }

    /// Draws a card. Busting loses the hand immediately.
    pub fn hit(&mut self) {
        if self.phase != Phase::PlayerTurn || self.game_over {
            return;
        }
        let card = self.draw();
        self.player.push(card);
        if hand_value(&self.player) > 21 {
            self.settle(Outcome::Lose);
        }
    }

    /// Sticks the hand; the dealer then draws to at least [`DEALER_STAND`].
    pub fn stand(&mut self) {
        if self.phase != Phase::PlayerTurn || self.game_over {
            return;
        }
        while hand_value(&self.dealer) < DEALER_STAND {
            let card = self.draw();
            self.dealer.push(card);
        }
        let player = hand_value(&self.player);
        let dealer = hand_value(&self.dealer);
        let outcome = if dealer > 21 || player > dealer {
            Outcome::Win
        } else if dealer == player {
            Outcome::Push
        } else {
            Outcome::Lose
        };
        self.settle(outcome);
    }

    /// Starts the next round after a settlement, or ends the run when the
    /// bet is no longer affordable.
    pub fn confirm(&mut self) {
        if self.game_over {
            return;
        }
        if self.chips < BET {
            self.game_over = true;
            return;
        }
        if self.phase == Phase::RoundOver {
            self.start_round();
        }
    }

    fn start_round(&mut self) {
        self.player.clear();
        self.dealer.clear();
        self.outcome = None;
        self.ensure_deck();
        for _ in 0..2 {
            let card = self.draw();
            self.player.push(card);
            let card = self.draw();
            self.dealer.push(card);
        }
        // Naturals settle immediately; otherwise the player acts.
        if is_blackjack(&self.player) || is_blackjack(&self.dealer) {
            let outcome = match (is_blackjack(&self.player), is_blackjack(&self.dealer)) {
                (true, true) => Outcome::Push,
                (true, false) => Outcome::Blackjack,
                (false, true) => Outcome::Lose,
                (false, false) => unreachable!("at least one natural"),
            };
            self.settle(outcome);
        } else {
            self.phase = Phase::PlayerTurn;
        }
    }

    fn ensure_deck(&mut self) {
        if self.deck.len() >= RESHUFFLE_AT {
            return;
        }
        let mut deck: Vec<Card> = Rank::ALL
            .iter()
            .flat_map(|rank| Suit::ALL.iter().map(move |suit| Card::new(*rank, *suit)))
            .collect();
        deck.shuffle(&mut self.rng);
        self.deck = deck;
    }

    fn draw(&mut self) -> Card {
        self.deck.pop().expect("deck is topped up by ensure_deck")
    }

    fn settle(&mut self, outcome: Outcome) {
        self.chips = self.chips.saturating_add_signed(payout(outcome));
        self.peak_chips = self.peak_chips.max(self.chips);
        match outcome {
            Outcome::Win | Outcome::Blackjack => self.wins += 1,
            Outcome::Lose => self.losses += 1,
            Outcome::Push => self.pushes += 1,
        }
        self.outcome = Some(outcome);
        if self.chips < BET {
            self.game_over = true;
        }
        self.phase = Phase::RoundOver;
    }

    // ---- accessors ---------------------------------------------------------

    pub fn is_game_over(&self) -> bool {
        self.game_over
    }

    /// The chip stack doubles as the run score for the high-score board.
    pub fn score(&self) -> u64 {
        self.peak_chips
    }

    /// The current chip stack.
    pub fn chips(&self) -> u64 {
        self.chips
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn outcome(&self) -> Option<Outcome> {
        self.outcome
    }

    pub fn player_hand(&self) -> &[Card] {
        &self.player
    }

    pub fn dealer_hand(&self) -> &[Card] {
        &self.dealer
    }

    pub fn player_value(&self) -> u8 {
        hand_value(&self.player)
    }

    /// The dealer's total, hidden until the player's turn is over.
    pub fn dealer_value(&self) -> Option<u8> {
        (!self.dealer_hole_hidden()).then(|| hand_value(&self.dealer))
    }

    /// Whether the dealer's hole card is still face down.
    pub fn dealer_hole_hidden(&self) -> bool {
        self.phase == Phase::PlayerTurn
    }

    pub fn wins(&self) -> u32 {
        self.wins
    }

    pub fn losses(&self) -> u32 {
        self.losses
    }

    pub fn pushes(&self) -> u32 {
        self.pushes
    }

    #[cfg(test)]
    pub(crate) fn force_chips(&mut self, chips: u64) {
        self.chips = chips;
    }

    /// Test hook: replaces both hands with known cards, resets the run to
    /// its fresh state and reopens the player turn, so flow tests never
    /// depend on shuffle order (an opening-deal natural would otherwise
    /// settle a round before the test acts).
    #[cfg(test)]
    pub(crate) fn debug_set_hands(&mut self, player: Vec<Card>, dealer: Vec<Card>) {
        self.player = player;
        self.dealer = dealer;
        self.chips = STARTING_CHIPS;
        self.peak_chips = STARTING_CHIPS;
        self.outcome = None;
        self.wins = 0;
        self.losses = 0;
        self.pushes = 0;
        self.game_over = false;
        self.phase = Phase::PlayerTurn;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn ace(suit: Suit) -> Card {
        Card::new(Rank::Ace, suit)
    }

    fn card(rank: Rank, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    #[test]
    fn fresh_game_deals_two_cards_each_and_has_the_budget() {
        let game = TwentyOne::new(7);
        assert_eq!(game.chips(), STARTING_CHIPS);
        assert_eq!(game.player_hand().len(), 2);
        assert_eq!(game.dealer_hand().len(), 2);
        assert!(game.player_value() <= 21);
        assert!(!game.is_game_over());
        assert_eq!(game.score(), STARTING_CHIPS);
    }

    #[test]
    fn dealer_hole_is_hidden_during_the_player_turn() {
        let game = TwentyOne::new(7);
        assert!(game.dealer_hole_hidden());
        assert_eq!(game.dealer_value(), None);
    }

    #[test]
    fn stand_always_reaches_a_verdict() {
        for seed in 0..64 {
            let mut game = TwentyOne::new(seed);
            let player_turn = game.phase() == Phase::PlayerTurn;
            game.stand();
            assert_eq!(game.phase(), Phase::RoundOver, "seed {seed}");
            let outcome = game.outcome().expect("a settled round has an outcome");
            assert_eq!(
                game.chips(),
                STARTING_CHIPS.saturating_add_signed(payout(outcome)),
                "seed {seed}"
            );
            // Only rounds the player actually stuck on run the dealer draw;
            // naturals settle immediately.
            if player_turn {
                let dealer = game.dealer_value().expect("hole is revealed");
                assert!(dealer >= DEALER_STAND, "seed {seed}: dealer {dealer}");
            }
        }
    }

    #[test]
    fn hit_draws_until_the_player_acts_or_busts() {
        let mut game = TwentyOne::new(3);
        game.debug_set_hands(
            vec![
                card(Rank::Two, Suit::Clubs),
                card(Rank::Three, Suit::Hearts),
            ],
            vec![
                card(Rank::Ten, Suit::Spades),
                card(Rank::Ten, Suit::Diamonds),
            ],
        );
        game.hit();
        assert_eq!(game.player_hand().len(), 3);
        // The drawn card is at most a ten or ace: 5 + 11 = 16, never a bust.
        assert!(game.player_value() <= 16);
        assert_eq!(game.phase(), Phase::PlayerTurn);
    }

    #[test]
    fn busting_loses_the_round_immediately() {
        let mut game = TwentyOne::new(3);
        game.debug_set_hands(
            vec![
                card(Rank::Ten, Suit::Clubs),
                card(Rank::Five, Suit::Hearts),
                card(Rank::Ten, Suit::Spades),
            ],
            vec![
                card(Rank::Ten, Suit::Diamonds),
                card(Rank::Seven, Suit::Clubs),
            ],
        );
        game.hit();
        assert_eq!(game.phase(), Phase::RoundOver);
        assert_eq!(game.outcome(), Some(Outcome::Lose));
        assert_eq!(game.chips(), STARTING_CHIPS - BET);
        assert_eq!(game.losses(), 1);
    }

    #[test]
    fn dealer_bust_pays_the_player() {
        let mut game = TwentyOne::new(3);
        game.debug_set_hands(
            vec![
                card(Rank::Nine, Suit::Clubs),
                card(Rank::Nine, Suit::Hearts),
            ],
            vec![
                card(Rank::Ten, Suit::Spades),
                card(Rank::Seven, Suit::Diamonds),
                card(Rank::Ten, Suit::Clubs),
            ],
        );
        game.stand();
        assert_eq!(game.outcome(), Some(Outcome::Win));
        assert_eq!(game.chips(), STARTING_CHIPS + BET);
        assert_eq!(game.wins(), 1);
    }

    #[test]
    fn equal_totals_push() {
        let mut game = TwentyOne::new(3);
        game.debug_set_hands(
            vec![
                card(Rank::Ten, Suit::Clubs),
                card(Rank::Eight, Suit::Hearts),
            ],
            vec![
                card(Rank::Ten, Suit::Spades),
                card(Rank::Eight, Suit::Diamonds),
            ],
        );
        game.stand();
        assert_eq!(game.outcome(), Some(Outcome::Push));
        assert_eq!(game.chips(), STARTING_CHIPS);
        assert_eq!(game.pushes(), 1);
    }

    #[test]
    fn payouts_match_the_outcome_table() {
        assert_eq!(payout(Outcome::Win), 100);
        assert_eq!(payout(Outcome::Blackjack), 150);
        assert_eq!(payout(Outcome::Lose), -100);
        assert_eq!(payout(Outcome::Push), 0);
    }

    #[test]
    fn hand_value_handles_aces() {
        assert_eq!(
            hand_value(&[ace(Suit::Clubs), card(Rank::Ten, Suit::Hearts)]),
            21
        );
        assert_eq!(
            hand_value(&[ace(Suit::Clubs), card(Rank::Five, Suit::Hearts)]),
            16
        );
        assert_eq!(
            hand_value(&[
                ace(Suit::Clubs),
                ace(Suit::Diamonds),
                card(Rank::Nine, Suit::Hearts)
            ]),
            21
        );
        assert_eq!(
            hand_value(&[
                card(Rank::Ten, Suit::Clubs),
                card(Rank::Ten, Suit::Hearts),
                card(Rank::Five, Suit::Spades)
            ]),
            25
        );
    }

    #[test]
    fn blackjack_detection_is_exact() {
        assert!(is_blackjack(&[
            ace(Suit::Clubs),
            card(Rank::King, Suit::Hearts)
        ]));
        assert!(!is_blackjack(&[
            card(Rank::Ten, Suit::Clubs),
            card(Rank::Five, Suit::Hearts),
            card(Rank::Six, Suit::Spades),
        ]));
        assert!(!is_blackjack(&[
            card(Rank::Ten, Suit::Clubs),
            card(Rank::Ace, Suit::Hearts),
            card(Rank::Ace, Suit::Spades)
        ]));
    }

    #[test]
    fn seeded_games_deal_identically() {
        let a = TwentyOne::new(42);
        let b = TwentyOne::new(42);
        assert_eq!(a.player_hand(), b.player_hand());
        assert_eq!(a.dealer_hand(), b.dealer_hand());
    }

    #[test]
    fn shoe_reshuffles_when_low() {
        let mut game = TwentyOne::new(9);
        game.stand(); // settle whatever was dealt
        game.deck.truncate(4);
        game.confirm();
        assert_eq!(game.player_hand().len(), 2);
        assert_eq!(game.dealer_hand().len(), 2);
        assert_eq!(game.deck.len(), 52 - 4);
    }

    #[test]
    fn game_over_when_the_bet_is_unaffordable() {
        let mut game = TwentyOne::new(9);
        game.force_chips(BET - 1);
        game.confirm();
        assert!(game.is_game_over());
    }

    #[test]
    fn input_outside_the_player_turn_is_ignored() {
        let mut game = TwentyOne::new(9);
        game.stand();
        let chips = game.chips();
        let hands = (game.player_hand().to_vec(), game.dealer_hand().to_vec());
        game.hit();
        game.stand();
        assert_eq!(game.chips(), chips);
        assert_eq!(game.player_hand(), &hands.0[..]);
        assert_eq!(game.dealer_hand(), &hands.1[..]);
        // A confirmed round deals fresh hands and reopens the turn (unless a
        // natural settles it — the hand lengths are the invariant).
        game.confirm();
        assert_eq!(game.player_hand().len(), 2);
        assert_eq!(game.dealer_hand().len(), 2);
    }

    #[test]
    fn update_is_a_no_op() {
        let mut game = TwentyOne::new(9);
        let before = (
            game.chips(),
            game.phase(),
            game.player_hand().to_vec(),
            game.dealer_hand().to_vec(),
        );
        game.update(Duration::from_secs(5));
        assert_eq!(game.chips(), before.0);
        assert_eq!(game.phase(), before.1);
        assert_eq!(game.player_hand(), &before.2[..]);
        assert_eq!(game.dealer_hand(), &before.3[..]);
    }
}
