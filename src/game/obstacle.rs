use crate::game::collision::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObstacleKind {
    /// One cell wide, two tall — easy to hop.
    Small,
    /// Three cells wide, three tall — needs a well-timed jump.
    Large,
}

impl ObstacleKind {
    pub fn width(self) -> u16 {
        match self {
            Self::Small => 1,
            Self::Large => 3,
        }
    }

    pub fn height(self) -> u16 {
        match self {
            Self::Small => 2,
            Self::Large => 3,
        }
    }
}

/// An obstacle moving toward the player. `x` is the distance in world cells
/// from the player's left edge; obstacles sit on the ground (y = 0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Obstacle {
    pub x: f64,
    pub kind: ObstacleKind,
}

impl Obstacle {
    pub fn new(x: f64, kind: ObstacleKind) -> Self {
        Self { x, kind }
    }

    pub fn width(&self) -> f64 {
        f64::from(self.kind.width())
    }

    pub fn height(&self) -> f64 {
        f64::from(self.kind.height())
    }

    pub fn rect(&self) -> Rect {
        Rect::new(self.x, 0.0, self.width(), self.height())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_obstacle_dimensions() {
        let obstacle = Obstacle::new(10.0, ObstacleKind::Small);
        assert_eq!(obstacle.width(), 1.0);
        assert_eq!(obstacle.height(), 2.0);
    }

    #[test]
    fn large_obstacle_dimensions() {
        let obstacle = Obstacle::new(10.0, ObstacleKind::Large);
        assert_eq!(obstacle.width(), 3.0);
        assert_eq!(obstacle.height(), 3.0);
    }

    #[test]
    fn rect_sits_on_the_ground() {
        let obstacle = Obstacle::new(7.5, ObstacleKind::Large);
        let rect = obstacle.rect();
        assert_eq!(rect.x, 7.5);
        assert_eq!(rect.y, 0.0);
        assert_eq!(rect.w, 3.0);
        assert_eq!(rect.h, 3.0);
    }
}
