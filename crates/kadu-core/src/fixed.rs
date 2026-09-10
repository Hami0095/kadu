//! Q16.16 fixed-point arithmetic on i32. No floats anywhere.

use std::cmp::Ordering;
use std::fmt;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

pub const FRAC_BITS: i32 = 16;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Fixed(pub i32);

impl Fixed {
    pub const ZERO: Fixed = Fixed(0);
    pub const ONE: Fixed = Fixed(1 << FRAC_BITS);

    /// Construct from a raw integer number of units (whole numbers only).
    pub const fn from_int(v: i32) -> Fixed {
        Fixed(v << FRAC_BITS)
    }

    /// Construct from a raw Q16.16 bit pattern.
    pub const fn from_raw(v: i32) -> Fixed {
        Fixed(v)
    }

    pub const fn raw(self) -> i32 {
        self.0
    }

    /// Truncating integer part.
    pub const fn to_int(self) -> i32 {
        self.0 >> FRAC_BITS
    }

    pub fn add(self, other: Fixed) -> Fixed {
        Fixed(self.0.wrapping_add(other.0))
    }

    pub fn sub(self, other: Fixed) -> Fixed {
        Fixed(self.0.wrapping_sub(other.0))
    }

    pub fn mul(self, other: Fixed) -> Fixed {
        let a = self.0 as i64;
        let b = other.0 as i64;
        let r = (a * b) >> FRAC_BITS;
        Fixed(r as i32)
    }

    pub fn div(self, other: Fixed) -> Fixed {
        debug_assert!(other.0 != 0, "Fixed division by zero");
        let a = (self.0 as i64) << FRAC_BITS;
        let b = other.0 as i64;
        Fixed((a / b) as i32)
    }

    pub fn abs(self) -> Fixed {
        Fixed(self.0.wrapping_abs())
    }

    pub fn cmp(self, other: Fixed) -> Ordering {
        self.0.cmp(&other.0)
    }

    pub fn min(self, other: Fixed) -> Fixed {
        if self.cmp(other) == Ordering::Less { self } else { other }
    }

    pub fn max(self, other: Fixed) -> Fixed {
        if self.cmp(other) == Ordering::Greater { self } else { other }
    }

    pub fn clamp(self, lo: Fixed, hi: Fixed) -> Fixed {
        self.max(lo).min(hi)
    }

    pub fn is_negative(self) -> bool {
        self.0 < 0
    }

    pub fn signum(self) -> i32 {
        self.0.signum()
    }

    /// Multiply by an integer numerator/denominator pair (used for percentage scaling)
    /// without ever introducing a float. Rounds toward zero.
    pub fn mul_pct(self, pct: i32) -> Fixed {
        let v = (self.0 as i64) * (pct as i64) / 100;
        Fixed(v as i32)
    }
}

impl Add for Fixed {
    type Output = Fixed;
    fn add(self, rhs: Fixed) -> Fixed {
        Fixed::add(self, rhs)
    }
}
impl Sub for Fixed {
    type Output = Fixed;
    fn sub(self, rhs: Fixed) -> Fixed {
        Fixed::sub(self, rhs)
    }
}
impl Mul for Fixed {
    type Output = Fixed;
    fn mul(self, rhs: Fixed) -> Fixed {
        Fixed::mul(self, rhs)
    }
}
impl Div for Fixed {
    type Output = Fixed;
    fn div(self, rhs: Fixed) -> Fixed {
        Fixed::div(self, rhs)
    }
}
impl Neg for Fixed {
    type Output = Fixed;
    fn neg(self) -> Fixed {
        Fixed(-self.0)
    }
}
impl AddAssign for Fixed {
    fn add_assign(&mut self, rhs: Fixed) {
        *self = *self + rhs;
    }
}
impl SubAssign for Fixed {
    fn sub_assign(&mut self, rhs: Fixed) {
        *self = *self - rhs;
    }
}
impl PartialOrd for Fixed {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.0.cmp(&other.0))
    }
}
impl Ord for Fixed {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}
impl fmt::Debug for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.0 >> FRAC_BITS;
        let frac = (self.0 & 0xFFFF) as i64 * 10000 / 65536;
        write!(f, "{}.{:04}", whole, frac.abs())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Debug)]
pub struct Vec2 {
    pub x: Fixed,
    pub y: Fixed,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: Fixed::ZERO, y: Fixed::ZERO };

    pub fn new(x: Fixed, y: Fixed) -> Vec2 {
        Vec2 { x, y }
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self.x + rhs.x, self.y + rhs.y)
    }
}
impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self.x - rhs.x, self.y - rhs.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_int_roundtrip() {
        assert_eq!(Fixed::from_int(5).to_int(), 5);
        assert_eq!(Fixed::from_int(-3).to_int(), -3);
        assert_eq!(Fixed::from_int(0).to_int(), 0);
    }

    #[test]
    fn add_sub() {
        let a = Fixed::from_int(3);
        let b = Fixed::from_int(2);
        assert_eq!((a + b).to_int(), 5);
        assert_eq!((a - b).to_int(), 1);
    }

    #[test]
    fn mul_basic() {
        let a = Fixed::from_int(3);
        let b = Fixed::from_int(4);
        assert_eq!((a * b).to_int(), 12);
        let half = Fixed::from_raw(1 << 15); // 0.5
        assert_eq!((Fixed::from_int(10) * half).to_int(), 5);
    }

    #[test]
    fn div_basic() {
        let a = Fixed::from_int(10);
        let b = Fixed::from_int(4);
        let r = a / b;
        // 2.5
        assert_eq!(r.raw(), (5 << 16) / 2);
    }

    #[test]
    fn abs_and_cmp() {
        let a = Fixed::from_int(-5);
        assert_eq!(a.abs().to_int(), 5);
        assert!(Fixed::from_int(1) < Fixed::from_int(2));
    }

    #[test]
    fn clamp_works() {
        let v = Fixed::from_int(150);
        let clamped = v.clamp(Fixed::from_int(0), Fixed::from_int(100));
        assert_eq!(clamped.to_int(), 100);
    }

    #[test]
    fn mul_pct_rounds_toward_zero() {
        let v = Fixed::from_int(100);
        assert_eq!(v.mul_pct(90).to_int(), 90);
        assert_eq!(v.mul_pct(50).to_int(), 50);
    }

    #[test]
    fn deterministic_across_repeated_runs() {
        // Same operations must produce bit-identical results every time.
        let mut acc = Fixed::from_int(1);
        for i in 1..1000 {
            acc = acc + Fixed::from_int(i) * Fixed::from_raw(3) - Fixed::from_int(1);
        }
        let mut acc2 = Fixed::from_int(1);
        for i in 1..1000 {
            acc2 = acc2 + Fixed::from_int(i) * Fixed::from_raw(3) - Fixed::from_int(1);
        }
        assert_eq!(acc.raw(), acc2.raw());
    }
}
