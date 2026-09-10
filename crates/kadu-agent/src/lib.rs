use kadu_core::{AttackKind, Direction, FighterState, Intent, Observation};
pub use kadu_core::{Agent, Fixed};

/// A punching bag. Always does nothing. Used as a fixture and in tests.
pub struct Dummy;

impl Agent for Dummy {
    fn name(&self) -> &str {
        "dummy"
    }

    fn decide(&mut self, _obs: &Observation) -> Intent {
        Intent::None
    }

    fn reset(&mut self) {}
}

/// Deliberately simple and deliberately beatable: walks forward, throws a
/// Light when in range, blocks when the opponent is in AttackStartup.
pub struct Rusher {
    light_range: Fixed,
}

impl Rusher {
    pub fn new() -> Rusher {
        Rusher { light_range: Fixed::from_int(140) }
    }
}

impl Default for Rusher {
    fn default() -> Self {
        Rusher::new()
    }
}

impl Agent for Rusher {
    fn name(&self) -> &str {
        "rusher"
    }

    fn decide(&mut self, obs: &Observation) -> Intent {
        if obs.opponent.state == FighterState::AttackStartup {
            return Intent::Block;
        }
        if obs.distance <= self.light_range {
            return Intent::Attack(AttackKind::Light);
        }
        Intent::Move(Direction::Forward)
    }

    fn reset(&mut self) {}
}
