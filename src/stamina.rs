//! Flight stamina of a pigeon.

use bevy::prelude::*;

/// Stamina as a fraction: 0 is empty, 1 is full.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Stamina(f32);

impl Default for Stamina {
    fn default() -> Self {
        Self::FULL
    }
}

impl Stamina {
    pub const FULL: Self = Self(1.0);

    pub fn new(value: f32) -> Self {
        Self(value.clamp(0.0, 1.0))
    }

    pub fn value(self) -> f32 {
        self.0
    }

    pub fn is_empty(self) -> bool {
        self.0 <= 0.0
    }

    /// Spends `amount` only if all of it is available, so a flap is never partial.
    pub fn try_spend(&mut self, amount: f32) -> bool {
        if self.0 < amount {
            return false;
        }
        self.0 -= amount;
        true
    }

    pub fn drain(&mut self, amount: f32) {
        self.0 = (self.0 - amount).max(0.0);
    }

    pub fn refill(&mut self, amount: f32) {
        self.0 = (self.0 + amount).min(1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_clamps_to_range() {
        assert_eq!(Stamina::new(1.5).value(), 1.0);
        assert_eq!(Stamina::new(-0.5).value(), 0.0);
    }

    #[test]
    fn try_spend_takes_amount() {
        let mut stamina = Stamina::new(0.5);
        assert!(stamina.try_spend(0.2));
        assert!((stamina.value() - 0.3).abs() < 1e-6);
    }

    #[test]
    fn try_spend_refuses_more_than_available() {
        let mut stamina = Stamina::new(0.1);
        assert!(!stamina.try_spend(0.2));
        assert_eq!(stamina.value(), 0.1);
    }

    #[test]
    fn drain_stops_at_zero() {
        let mut stamina = Stamina::new(0.1);
        stamina.drain(0.5);
        assert!(stamina.is_empty());
    }

    #[test]
    fn refill_stops_at_full() {
        let mut stamina = Stamina::new(0.9);
        stamina.refill(0.5);
        assert_eq!(stamina, Stamina::FULL);
    }
}
