/// Game state within a single run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Playing,
    GameOver,
}
