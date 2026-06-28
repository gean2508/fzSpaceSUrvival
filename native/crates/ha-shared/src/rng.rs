//! Deterministic mulberry32 — port of `packages/shared/src/sim/rng.ts`.

pub struct Rng {
    state: u32,
}

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// Returns float in [0, 1).
    pub fn next(&mut self) -> f32 {
        self.state = self.state.wrapping_add(0x6d2b79f5);
        let mut t = (self.state ^ (self.state >> 15)).wrapping_mul(1 | self.state);
        t = (t ^ (t >> 7)).wrapping_mul(61 | t) ^ t;
        ((t ^ (t >> 14)) as f32) / 4294967296.0
    }

    pub fn range(&mut self, min: f32, max: f32) -> f32 {
        min + self.next() * (max - min)
    }
}
