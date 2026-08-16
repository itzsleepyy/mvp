use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use crate::game::obstacle::{Obstacle, ObstacleKind};

/// Base world speed in cells per second.
pub const BASE_SPEED: f64 = 16.0;

/// Minimum reaction window between obstacles, in seconds. Multiplied by the
/// current speed to get the minimum distance gap, which guarantees obstacles
/// are always physically possible to clear.
const REACTION_SECS: f64 = 0.8;
const GAP_EXTRA_MIN: f64 = 4.0;
const GAP_EXTRA_MAX: f64 = 14.0;
/// Distance past the left edge before an obstacle is removed.
const CULL_MARGIN: f64 = 2.0;
/// Guard against enormous frame gaps; the game caps at 50ms anyway.
const MAX_STEP: f64 = 0.1;

/// The scrolling world: obstacle generation, movement and difficulty scaling.
pub struct World {
    pub(crate) obstacles: Vec<Obstacle>,
    rng: StdRng,
    speed: f64,
    elapsed: f64,
    distance_since_spawn: f64,
    next_gap: f64,
    pub(crate) spawn_x: f64,
    player_col: u16,
}

impl World {
    /// Creates a world with a seeded RNG for reproducible tests. `player_col`
    /// and `playfield_cols` describe the playable viewport in cells.
    pub fn new(seed: u64, player_col: u16, playfield_cols: u16) -> Self {
        let mut world = Self {
            obstacles: Vec::new(),
            rng: StdRng::seed_from_u64(seed),
            speed: BASE_SPEED,
            elapsed: 0.0,
            distance_since_spawn: 0.0,
            next_gap: BASE_SPEED, // grace period before the first obstacle
            spawn_x: 0.0,
            player_col,
        };
        world.set_viewport(player_col, playfield_cols);
        world
    }

    /// Updates the viewport geometry. Spawns happen just off the right edge.
    pub fn set_viewport(&mut self, player_col: u16, playfield_cols: u16) {
        self.player_col = player_col;
        self.spawn_x = f64::from(playfield_cols.saturating_sub(player_col)) + 1.0;
    }

    /// Advances the world by `dt` seconds and returns the number of obstacles
    /// that were passed this step (they left the left edge).
    pub fn update(&mut self, dt: f64) -> usize {
        let dt = dt.clamp(0.0, MAX_STEP);
        self.elapsed += dt;
        self.speed = self.speed_at(self.elapsed);

        let travelled = self.speed * dt;
        self.distance_since_spawn += travelled;
        for obstacle in &mut self.obstacles {
            obstacle.x -= travelled;
        }

        let mut passed = 0;
        self.obstacles.retain(|obstacle| {
            let keep = obstacle.x + obstacle.width() > -CULL_MARGIN;
            if !keep {
                passed += 1;
            }
            keep
        });

        if self.distance_since_spawn >= self.next_gap {
            self.distance_since_spawn -= self.next_gap;
            self.spawn();
            self.next_gap = self.roll_gap();
        }
        passed
    }

    /// Speed multiplier grows smoothly with elapsed time, approaching a cap.
    pub fn speed_multiplier(&self) -> f64 {
        self.speed / BASE_SPEED
    }

    fn speed_at(&self, elapsed: f64) -> f64 {
        let multiplier = 1.0 + 1.2 * (1.0 - 1.0 / (1.0 + elapsed / 90.0));
        BASE_SPEED * multiplier
    }

    fn spawn(&mut self) {
        let kind = self.roll_kind();
        self.obstacles.push(Obstacle::new(self.spawn_x, kind));
    }

    /// Large obstacles become more likely over time.
    fn roll_kind(&mut self) -> ObstacleKind {
        let large_prob = (0.10 + self.elapsed / 90.0).clamp(0.10, 0.55);
        if self.rng.random::<f64>() < large_prob {
            ObstacleKind::Large
        } else {
            ObstacleKind::Small
        }
    }

    /// Distance to the next spawn: always at least `speed * REACTION_SECS`
    /// so the player can always land and react between obstacles.
    fn roll_gap(&mut self) -> f64 {
        self.speed * REACTION_SECS + self.rng.random_range(GAP_EXTRA_MIN..GAP_EXTRA_MAX)
    }

    pub fn obstacles(&self) -> &[Obstacle] {
        &self.obstacles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAYER_COL: u16 = 11;
    const COLS: u16 = 58;
    const DT: f64 = 1.0 / 60.0;

    /// Steps the world, recording `(gap, speed)` for every spawn after the
    /// first, where `gap` is the distance to the previous obstacle at the
    /// moment of spawning and `speed` is the speed at spawn time. A freshly
    /// spawned obstacle sits exactly at `spawn_x` because movement is applied
    /// before spawning within each update.
    fn record_spawn_gaps(world: &mut World, seconds: f64) -> Vec<(f64, f64)> {
        let mut spawns = Vec::new();
        let steps = (seconds / DT) as usize;
        for _ in 0..steps {
            world.update(DT);
            let obstacles = world.obstacles();
            let Some(newest) = obstacles.last() else {
                continue;
            };
            if newest.x == world.spawn_x {
                let gap = match obstacles.len() {
                    0 | 1 => continue, // first spawn: no predecessor yet
                    len => newest.x - obstacles[len - 2].x,
                };
                spawns.push((gap, world.speed));
            }
        }
        spawns
    }

    #[test]
    fn obstacles_move_left_at_current_speed() {
        let mut world = World::new(7, PLAYER_COL, COLS);
        let mut steps = 0;
        while world.obstacles().is_empty() {
            world.update(DT);
            steps += 1;
            assert!(steps < 10_000, "first obstacle never spawned");
        }
        let before: Vec<f64> = world.obstacles().iter().map(|o| o.x).collect();
        let speed = world.speed;
        world.update(0.1);
        for (before, obstacle) in before.iter().zip(world.obstacles()) {
            // Speed grows slightly during the step; allow a small tolerance.
            assert!((obstacle.x - (before - speed * 0.1)).abs() < 0.01);
            assert!(obstacle.x < *before);
        }
    }

    #[test]
    fn off_screen_obstacles_are_removed() {
        let mut world = World::new(11, PLAYER_COL, COLS);
        record_spawn_gaps(&mut world, 30.0);
        for obstacle in world.obstacles() {
            assert!(obstacle.x + obstacle.width() > -CULL_MARGIN);
        }
    }

    #[test]
    fn spawn_spacing_always_allows_a_reaction_window() {
        let mut world = World::new(42, PLAYER_COL, COLS);
        let spawns = record_spawn_gaps(&mut world, 120.0);
        assert!(
            spawns.len() >= 30,
            "expected more spawns, got {}",
            spawns.len()
        );
        for (gap, speed_at_spawn) in spawns {
            assert!(
                gap >= speed_at_spawn * REACTION_SECS,
                "gap {gap} too small at speed {speed_at_spawn}"
            );
        }
    }

    #[test]
    fn first_obstacle_spawns_off_the_right_edge() {
        let mut world = World::new(3, PLAYER_COL, COLS);
        let mut steps = 0;
        while world.obstacles().is_empty() {
            world.update(DT);
            steps += 1;
            assert!(steps < 10_000, "first obstacle never spawned");
        }
        let newest = world.obstacles().last().unwrap();
        // One cell beyond the rightmost playfield column.
        assert_eq!(newest.x, f64::from(COLS - PLAYER_COL) + 1.0);
        assert!(newest.x > f64::from(COLS - PLAYER_COL - 1));
    }

    #[test]
    fn speed_increases_smoothly_and_stays_bounded() {
        let mut world = World::new(5, PLAYER_COL, COLS);
        let mut speeds = vec![world.speed];
        for _ in 0..(120.0 / DT) as usize {
            world.update(DT);
            speeds.push(world.speed);
        }
        let first = *speeds.first().unwrap();
        let last = *speeds.last().unwrap();
        assert_eq!(first, BASE_SPEED);
        assert!(last > BASE_SPEED * 1.4);
        assert!(last < BASE_SPEED * 2.3);
        for pair in speeds.windows(2) {
            assert!(pair[1] >= pair[0]);
        }
    }

    #[test]
    fn update_with_huge_dt_does_not_run_away() {
        let mut world = World::new(5, PLAYER_COL, COLS);
        world.update(1000.0);
        assert!(world.obstacles().len() <= 1);
    }

    #[test]
    fn passed_count_reports_obstacles_exiting_left() {
        let mut world = World::new(5, PLAYER_COL, COLS);
        record_spawn_gaps(&mut world, 30.0);
        assert!(!world.obstacles().is_empty());
        let mut passed = 0;
        for _ in 0..(60.0 / DT) as usize {
            passed += world.update(DT);
        }
        assert!(passed > 0);
    }
}
