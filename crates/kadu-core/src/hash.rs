//! 64-bit FNV-1a hashing of world state, chained tick over tick.

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

pub struct Fnv1a(u64);

impl Fnv1a {
    pub fn new() -> Fnv1a {
        Fnv1a(FNV_OFFSET)
    }

    pub fn write_u8(&mut self, b: u8) {
        self.0 ^= b as u64;
        self.0 = self.0.wrapping_mul(FNV_PRIME);
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u8(b);
        }
    }

    pub fn write_i32(&mut self, v: i32) {
        self.write_bytes(&v.to_le_bytes());
    }

    pub fn write_u32(&mut self, v: u32) {
        self.write_bytes(&v.to_le_bytes());
    }

    pub fn write_u64(&mut self, v: u64) {
        self.write_bytes(&v.to_le_bytes());
    }

    pub fn write_i64(&mut self, v: i64) {
        self.write_bytes(&v.to_le_bytes());
    }

    pub fn finish(self) -> u64 {
        self.0
    }
}

impl Default for Fnv1a {
    fn default() -> Self {
        Fnv1a::new()
    }
}

/// Chains the previous hash into the next tick's hash so the result depends
/// on the entire history, not just the current tick's state.
pub fn chain(prev: u64, tick_hash: u64) -> u64 {
    let mut h = Fnv1a::new();
    h.write_u64(prev);
    h.write_u64(tick_hash);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let mut a = Fnv1a::new();
        a.write_i32(42);
        a.write_u32(7);
        let mut b = Fnv1a::new();
        b.write_i32(42);
        b.write_u32(7);
        assert_eq!(a.finish(), b.finish());
    }

    #[test]
    fn order_matters() {
        let mut a = Fnv1a::new();
        a.write_i32(1);
        a.write_i32(2);
        let mut b = Fnv1a::new();
        b.write_i32(2);
        b.write_i32(1);
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn chain_accumulates() {
        let h1 = chain(0, 111);
        let h2 = chain(h1, 222);
        let h1_again = chain(0, 111);
        let h2_again = chain(h1_again, 222);
        assert_eq!(h2, h2_again);
    }
}
