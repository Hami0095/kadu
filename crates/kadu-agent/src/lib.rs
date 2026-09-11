use kadu_core::rng::Pcg32;
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

/// Falsifies Guard Crush. Blocks, and only blocks: never attacks, never
/// moves. Crouch-blocks when it last saw the opponent crouching (the only
/// observable proxy for "the incoming attack is low" - Observation
/// deliberately doesn't expose a move's crouching/jumping flag), stand-
/// blocks otherwise. If Turtle can win a round, blocking is free and the
/// anti-turtling rule is broken.
pub struct Turtle {
    low_incoming: bool,
}

impl Turtle {
    pub fn new() -> Turtle {
        Turtle { low_incoming: false }
    }
}

impl Default for Turtle {
    fn default() -> Self {
        Turtle::new()
    }
}

impl Agent for Turtle {
    fn name(&self) -> &str {
        "turtle"
    }

    fn decide(&mut self, obs: &Observation) -> Intent {
        // Update our belief about the incoming attack's height only while
        // the opponent isn't already mid-swing, so we're reading their
        // stance, not a moment frozen inside their own attack animation.
        if !obs.opponent.state.is_attack() {
            self.low_incoming = obs.opponent.state == FighterState::Crouch;
        }
        let crouched = matches!(obs.me.state, FighterState::Crouch | FighterState::BlockCrouch);
        if self.low_incoming && !crouched {
            Intent::Crouch
        } else if !self.low_incoming && crouched {
            Intent::Move(Direction::Neutral)
        } else {
            Intent::Block
        }
    }

    fn reset(&mut self) {
        self.low_incoming = false;
    }
}

/// Falsifies the Passivity Count. Advances and attacks with Light until it
/// is ahead on vitality by any margin, then retreats toward its own corner
/// (Direction::Back always retreats: fighters always face each other) and
/// blocks for the rest of the round. If Runner wins reliably, "land one hit
/// and run" is the dominant strategy and the competition is dead on
/// arrival.
pub struct Runner {
    light_range: Fixed,
    ahead: bool,
}

impl Runner {
    pub fn new() -> Runner {
        Runner { light_range: Fixed::from_int(140), ahead: false }
    }
}

impl Default for Runner {
    fn default() -> Self {
        Runner::new()
    }
}

impl Agent for Runner {
    fn name(&self) -> &str {
        "runner"
    }

    fn decide(&mut self, obs: &Observation) -> Intent {
        if !self.ahead && obs.me.vitality > obs.opponent.vitality {
            self.ahead = true;
        }

        if self.ahead {
            if obs.my_corner_distance.to_int() > 80 {
                return Intent::Move(Direction::Back);
            }
            return Intent::Block;
        }

        if obs.opponent.state == FighterState::AttackStartup {
            return Intent::Block;
        }
        if obs.distance <= self.light_range {
            return Intent::Attack(AttackKind::Light);
        }
        Intent::Move(Direction::Forward)
    }

    fn reset(&mut self) {
        self.ahead = false;
    }
}

/// Falsifies combo damage scaling and the 15-hit cap (and reveals whether
/// Light is overtuned). Presses Light the instant it's actionable. No
/// blocking, no movement, no thought - the engine's own intent gating
/// handles "the instant it is actionable" for free.
pub struct Spammer;

impl Agent for Spammer {
    fn name(&self) -> &str {
        "spammer"
    }

    fn decide(&mut self, _obs: &Observation) -> Intent {
        Intent::Attack(AttackKind::Light)
    }

    fn reset(&mut self) {}
}

/// The baseline. Falsifies everything else: any agent that cannot beat
/// Random is broken, and Random beating Rusher means Rusher is broken.
///
/// Owns its own PCG32, entirely separate from the simulation's PRNG, seeded
/// deterministically from match config (`match_seed ^ slot_constant`) - never
/// from a system entropy source. Reading a non-deterministic source here is
/// the single most likely place for a silent replay-reproducibility bug to
/// enter the codebase, so this is deliberately the only place in kadu-agent
/// that touches randomness at all.
pub struct Random {
    rng: Pcg32,
}

/// XORed into the match seed per agent slot so two Random agents in the
/// same match (or the same agent seeded across different matches) don't
/// share a PRNG stream.
pub const RANDOM_AGENT_SLOT_CONSTANT: [u64; 2] = [0x9E37_79B9_7F4A_7C15, 0xC2B2_AE3D_27D4_EB4F];

const LEGAL_INTENTS: &[Intent] = &[
    Intent::None,
    Intent::Move(Direction::Neutral),
    Intent::Move(Direction::Forward),
    Intent::Move(Direction::Back),
    Intent::Move(Direction::Up),
    Intent::Move(Direction::Down),
    Intent::Move(Direction::UpForward),
    Intent::Move(Direction::UpBack),
    Intent::Move(Direction::DownForward),
    Intent::Move(Direction::DownBack),
    Intent::Attack(AttackKind::Light),
    Intent::Attack(AttackKind::Medium),
    Intent::Attack(AttackKind::Heavy),
    Intent::Block,
    Intent::Crouch,
    Intent::Dash(Direction::Forward),
    Intent::Dash(Direction::Back),
    Intent::Jump(Direction::Forward),
    Intent::Jump(Direction::Back),
    Intent::Throw,
];

impl Random {
    /// `match_seed` is the same seed the match was recorded with;
    /// `slot` is 0 or 1 (which fighter this agent is controlling).
    pub fn new(match_seed: u64, slot: usize) -> Random {
        let salt = RANDOM_AGENT_SLOT_CONSTANT[slot % 2];
        Random { rng: Pcg32::new(match_seed ^ salt, 1) }
    }
}

impl Agent for Random {
    fn name(&self) -> &str {
        "random"
    }

    fn decide(&mut self, _obs: &Observation) -> Intent {
        let idx = self.rng.next_bounded(LEGAL_INTENTS.len() as u32) as usize;
        LEGAL_INTENTS[idx]
    }

    fn reset(&mut self) {
        // Deliberately not reseeded: the PRNG stream is tied to the whole
        // match (via match_seed), not to any one round within it, and
        // continuing it across rounds is exactly as deterministic as
        // resetting it would be.
    }
}

/// v0.2 Change 5's payoff, not a v0.1 rule falsification. Plays position
/// rather than reaction: holds just outside Light range, walks forward and
/// back to bait a whiff, and only commits to an attack once the opponent
/// has already committed to one (state == AttackRecovery, i.e. can no
/// longer block or cancel). If Spacer can't beat Spammer once the neutral
/// game exists (v0.2 Change 5), spacing isn't actually rewarded and the
/// middle of the arena is still decoration - the most informative single
/// result the milestone can produce, per its own build prompt.
pub struct Spacer {
    /// Just past Light's reach: close enough to threaten and punish, far
    /// enough that a whiffed Light can't tag us for free.
    hold_range: Fixed,
    /// How far in/out of hold_range we tolerate before repositioning,
    /// versus baiting in place.
    band: Fixed,
    bait_forward: bool,
}

impl Spacer {
    pub fn new() -> Spacer {
        Spacer { hold_range: Fixed::from_int(155), band: Fixed::from_int(15), bait_forward: true }
    }
}

impl Default for Spacer {
    fn default() -> Self {
        Spacer::new()
    }
}

impl Agent for Spacer {
    fn name(&self) -> &str {
        "spacer"
    }

    fn decide(&mut self, obs: &Observation) -> Intent {
        // Punish: the opponent already committed to an attack that's now in
        // its unblockable, uncancellable recovery window, and we're close
        // enough to reach it.
        if obs.opponent.state == FighterState::AttackRecovery && obs.distance <= self.hold_range {
            return Intent::Attack(AttackKind::Light);
        }
        // Danger: the opponent is mid-swing and we're in range to eat it -
        // block rather than keep spacing.
        if obs.opponent.state == FighterState::AttackStartup && obs.distance <= self.hold_range {
            return Intent::Block;
        }

        let near = self.hold_range - self.band;
        let far = self.hold_range + self.band;
        if obs.distance < near {
            Intent::Move(Direction::Back)
        } else if obs.distance > far {
            Intent::Move(Direction::Forward)
        } else {
            // In the pocket: bait by rocking forward and back rather than
            // sitting still (a still target is a free read).
            self.bait_forward = !self.bait_forward;
            Intent::Move(if self.bait_forward { Direction::Forward } else { Direction::Back })
        }
    }

    fn reset(&mut self) {
        self.bait_forward = true;
    }
}
