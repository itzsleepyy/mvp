/// Axis-aligned rectangle in world coordinates. World units are terminal
/// cells; `x` grows to the right, `y` grows upward from the ground.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }
}

/// AABB intersection test. Two rectangles collide when their interiors
/// overlap; touching edges alone do not count as a collision.
pub fn intersects(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_rectangles_collide() {
        let a = Rect::new(0.0, 0.0, 2.0, 2.0);
        let b = Rect::new(1.0, 1.0, 2.0, 2.0);
        assert!(intersects(&a, &b));
        assert!(intersects(&b, &a));
    }

    #[test]
    fn separated_rectangles_do_not_collide() {
        let a = Rect::new(0.0, 0.0, 2.0, 2.0);
        let far_right = Rect::new(5.0, 0.0, 2.0, 2.0);
        let far_above = Rect::new(0.0, 6.0, 2.0, 2.0);
        assert!(!intersects(&a, &far_right));
        assert!(!intersects(&a, &far_above));
    }

    #[test]
    fn touching_edges_do_not_collide() {
        let a = Rect::new(0.0, 0.0, 2.0, 2.0);
        let beside = Rect::new(2.0, 0.0, 2.0, 2.0);
        let on_top = Rect::new(0.0, 2.0, 2.0, 2.0);
        assert!(!intersects(&a, &beside));
        assert!(!intersects(&a, &on_top));
    }

    #[test]
    fn identical_rectangles_collide() {
        let a = Rect::new(1.0, 1.0, 3.0, 4.0);
        assert!(intersects(&a, &a));
    }

    #[test]
    fn containment_collides() {
        let outer = Rect::new(0.0, 0.0, 10.0, 10.0);
        let inner = Rect::new(3.0, 3.0, 2.0, 2.0);
        assert!(intersects(&outer, &inner));
    }

    #[test]
    fn nearly_touching_edges_do_not_collide() {
        let a = Rect::new(0.0, 0.0, 2.0, 2.0);
        let near = Rect::new(2.001, 0.0, 2.0, 2.0);
        assert!(!intersects(&a, &near));
    }
}
