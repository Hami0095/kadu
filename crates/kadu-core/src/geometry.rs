use crate::fixed::{Fixed, Vec2};

/// Axis-aligned rectangle, offset relative to a fighter's origin (feet,
/// facing-forward-positive x). `y` grows upward from the floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: Fixed,
    pub y: Fixed,
    pub w: Fixed,
    pub h: Fixed,
}

impl Rect {
    /// Resolve this rectangle to world space given a fighter's origin and facing.
    /// Facing::Left mirrors the x offset.
    pub fn to_world(self, origin: Vec2, facing_right: bool) -> WorldRect {
        let x0 = if facing_right {
            origin.x + self.x
        } else {
            origin.x - self.x - self.w
        };
        WorldRect { x0, y0: origin.y + self.y, x1: x0 + self.w, y1: origin.y + self.y + self.h }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldRect {
    pub x0: Fixed,
    pub y0: Fixed,
    pub x1: Fixed,
    pub y1: Fixed,
}

impl WorldRect {
    pub fn overlaps(&self, other: &WorldRect) -> bool {
        self.x0 < other.x1 && other.x0 < self.x1 && self.y0 < other.y1 && other.y0 < self.y1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_detects_intersection() {
        let a = WorldRect {
            x0: Fixed::from_int(0),
            y0: Fixed::from_int(0),
            x1: Fixed::from_int(10),
            y1: Fixed::from_int(10),
        };
        let b = WorldRect {
            x0: Fixed::from_int(5),
            y0: Fixed::from_int(5),
            x1: Fixed::from_int(15),
            y1: Fixed::from_int(15),
        };
        assert!(a.overlaps(&b));
    }

    #[test]
    fn no_overlap_when_separated() {
        let a = WorldRect {
            x0: Fixed::from_int(0),
            y0: Fixed::from_int(0),
            x1: Fixed::from_int(10),
            y1: Fixed::from_int(10),
        };
        let b = WorldRect {
            x0: Fixed::from_int(20),
            y0: Fixed::from_int(20),
            x1: Fixed::from_int(30),
            y1: Fixed::from_int(30),
        };
        assert!(!a.overlaps(&b));
    }

    #[test]
    fn mirrors_for_facing_left() {
        let r = Rect {
            x: Fixed::from_int(10),
            y: Fixed::from_int(0),
            w: Fixed::from_int(5),
            h: Fixed::from_int(5),
        };
        let origin = Vec2::new(Fixed::from_int(100), Fixed::from_int(0));
        let right = r.to_world(origin, true);
        let left = r.to_world(origin, false);
        assert_eq!(right.x0.to_int(), 110);
        assert_eq!(left.x1.to_int(), 90);
    }
}
