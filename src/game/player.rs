use crate::game::collision::Rect;

/// Player dimensions in world cells.
pub const WIDTH: f64 = 2.0;
pub const HEIGHT: f64 = 2.0;

/// Gravity pulls the player down, in cells per second squared.
pub const GRAVITY: f64 = 55.0;
/// Upward velocity applied on jump, in cells per second.
pub const JUMP_FORCE: f64 = 20.5;
/// Terminal fall speed keeps large frame gaps from tunneling through the ground.
const MAX_FALL_SPEED: f64 = 40.0;

/// The player's vertical state. The player is conceptually fixed at world
/// x = 0; the world scrolls past. `y` is the height above the ground in
/// cells, `grounded` is true while standing on the ground.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Player {
    pub y: f64,
    pub velocity: f64,
    pub grounded: bool,
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

impl Player {
    pub fn new() -> Self {
        Self {
            y: 0.0,
            velocity: 0.0,
            grounded: true,
        }
    }

    /// Jumps when grounded; ignored while airborne (no double jump).
    pub fn jump(&mut self) {
        if self.grounded {
            self.velocity = JUMP_FORCE;
            self.grounded = false;
        }
    }

    /// Advances physics by `dt` seconds using semi-implicit Euler. The player
    /// is clamped against the ground and can never fall through it, no matter
    /// how large `dt` is.
    pub fn update(&mut self, dt: f64) {
        if self.grounded {
            return;
        }
        self.velocity = (self.velocity - GRAVITY * dt).max(-MAX_FALL_SPEED);
        self.y += self.velocity * dt;
        if self.y <= 0.0 {
            self.y = 0.0;
            self.velocity = 0.0;
            self.grounded = true;
        }
    }

    pub fn rect(&self) -> Rect {
        Rect::new(0.0, self.y, WIDTH, HEIGHT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_grounded_at_ground_level() {
        let player = Player::new();
        assert!(player.grounded);
        assert_eq!(player.y, 0.0);
        assert_eq!(player.velocity, 0.0);
    }

    #[test]
    fn jump_when_grounded_launches_player() {
        let mut player = Player::new();
        player.jump();
        assert!(!player.grounded);
        assert_eq!(player.velocity, JUMP_FORCE);
    }

    #[test]
    fn cannot_jump_mid_air() {
        let mut player = Player::new();
        player.jump();
        player.update(0.1);
        assert!(!player.grounded);
        let airborne_velocity = player.velocity;
        player.jump();
        assert_eq!(player.velocity, airborne_velocity);
    }

    #[test]
    fn gravity_returns_player_to_ground() {
        let mut player = Player::new();
        player.jump();
        let mut steps = 0;
        while !player.grounded {
            player.update(1.0 / 60.0);
            steps += 1;
            assert!(steps < 10_000, "player never landed");
        }
        assert_eq!(player.y, 0.0);
        assert_eq!(player.velocity, 0.0);
    }

    #[test]
    fn does_not_fall_through_ground_on_large_step() {
        let mut player = Player::new();
        player.jump();
        player.update(10.0);
        assert!(player.grounded);
        assert_eq!(player.y, 0.0);
    }

    #[test]
    fn grounded_player_is_unaffected_by_update() {
        let mut player = Player::new();
        player.update(1.0);
        assert_eq!(player, Player::new());
    }

    #[test]
    fn jump_reaches_enough_height_to_clear_large_obstacles() {
        let mut player = Player::new();
        player.jump();
        let mut max_y: f64 = 0.0;
        while !player.grounded {
            player.update(1.0 / 60.0);
            max_y = max_y.max(player.y);
        }
        // A large obstacle is 3 cells tall; the player's apex must clear it.
        assert!(
            max_y > 3.0,
            "apex {max_y} too low to clear a large obstacle"
        );
    }
}
