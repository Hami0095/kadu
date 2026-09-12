use kadu_core::rng::Pcg32;
use kadu_core::{AttackKind, Direction, FighterState, Intent, Observation};
pub use kadu_core::{Agent, Fixed};

/// Shared stalemate-breaker for fixtures that never initiate an attack
/// under their own normal logic (Turtle: never attacks, period; Spacer:
/// only ever attacks off a specific reactive signal). Two such fixtures
/// paired against each other can otherwise stand at point-blank range for
/// an entire round without ever landing a hit - not because the ruleset
/// produced a timeout, but because neither fixture's decision function
/// contains a path that throws a punch. That's a fixture-contact failure,
/// not a finding, and it was invisible until contested-round partitioning
/// made "0 hits, round timed out" visible as distinct from a real fight.
///
/// Tracks ticks since either fighter's vitality last changed (the
/// observable proxy for "a hit landed") and reports true once that's gone
/// on long enough, while in range, that a real fight would almost
/// certainly already show some damage. Firing it throws exactly one
/// probing Light and lets the fixture's own logic resume immediately
/// after - this breaks deadlocks without redefining what the fixture
/// normally does.
struct ProbeTimer {
    last_my_vitality: i32,
    last_opponent_vitality: i32,
    stale_polls: u32,
    threshold_polls: u32,
}

impl ProbeTimer {
    /// `threshold_polls` is in units of `decide()` calls (one per
    /// `reflex_interval` ticks), not raw ticks.
    fn new(threshold_polls: u32) -> ProbeTimer {
        ProbeTimer { last_my_vitality: i32::MIN, last_opponent_vitality: i32::MIN, stale_polls: 0, threshold_polls }
    }

    /// Call once per `decide()`. Returns true when the stalemate threshold
    /// has just been reached (fires once per stale streak, not on every
    /// poll past the threshold, so callers can treat it as "probe now").
    fn should_probe(&mut self, obs: &Observation) -> bool {
        let unchanged = obs.me.vitality == self.last_my_vitality && obs.opponent.vitality == self.last_opponent_vitality;
        self.last_my_vitality = obs.me.vitality;
        self.last_opponent_vitality = obs.opponent.vitality;
        if unchanged {
            self.stale_polls += 1;
        } else {
            self.stale_polls = 0;
        }
        if self.stale_polls >= self.threshold_polls {
            // Reset rather than latch: if this probe whiffs (e.g. the
            // opponent isn't actually in hurtbox reach despite being
            // within our coarse range check), vitality stays unchanged
            // and this fires again after another full threshold - a
            // periodic retry, not a one-shot that can silently give up
            // for the rest of the round.
            self.stale_polls = 0;
            true
        } else {
            false
        }
    }

    fn reset(&mut self) {
        self.last_my_vitality = i32::MIN;
        self.last_opponent_vitality = i32::MIN;
        self.stale_polls = 0;
    }
}

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

/// Falsifies Guard Crush. Blocks once in range - never attacks - crouch-
/// blocking when it last saw the opponent crouching (the only observable
/// proxy for "the incoming attack is low" - Observation deliberately
/// doesn't expose a move's crouching/jumping flag), stand-blocking
/// otherwise. If Turtle can win a round, blocking is free and the
/// anti-turtling rule is broken.
///
/// Walks forward when out of range, and throws one probing Light if
/// nobody's vitality has moved for a while despite being in range (see
/// `ProbeTimer`) - otherwise two Turtles, or a Turtle and a Dummy, stand at
/// point-blank range for a whole round without a single hit, since neither
/// side's normal logic contains a path that attacks. Earlier versions did
/// neither of these, which is a fixture bug, not a finding: two agents
/// that never make contact simply never test anything, and a tournament
/// that mixes those non-engagements in with real fights silently launders
/// "the fixtures never made contact" into "the ruleset produces lots of
/// timeouts." Both additions are stalemate-breakers only - Turtle still
/// never attacks as its own first choice, and still blocks everything it
/// can once engaged.
pub struct Turtle {
    low_incoming: bool,
    approach_range: Fixed,
    probe: ProbeTimer,
}

impl Turtle {
    pub fn new() -> Turtle {
        Turtle { low_incoming: false, approach_range: Fixed::from_int(160), probe: ProbeTimer::new(60) }
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
        // Called unconditionally so the stalemate clock keeps running even
        // while approaching, not just while already in blocking range.
        let should_probe = self.probe.should_probe(obs);

        if obs.distance > self.approach_range && !obs.opponent.state.is_attack() {
            return Intent::Move(Direction::Forward);
        }
        if should_probe {
            return Intent::Attack(AttackKind::Light);
        }
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
        self.probe.reset();
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
/// Light is overtuned). Presses Light the instant it's actionable, once in
/// range. No blocking, no thought otherwise - the engine's own intent
/// gating handles "the instant it is actionable" for free.
///
/// Walks forward when out of range. Earlier versions never moved at all,
/// relying entirely on the opponent (or a close enough starting position)
/// to bring them into contact; against a stationary opponent that never
/// happened, and the resulting non-engagement was previously misread as a
/// finding rather than the fixture-contact bug it is.
pub struct Spammer {
    approach_range: Fixed,
}

impl Spammer {
    pub fn new() -> Spammer {
        Spammer { approach_range: Fixed::from_int(140) }
    }
}

impl Default for Spammer {
    fn default() -> Self {
        Spammer::new()
    }
}

impl Agent for Spammer {
    fn name(&self) -> &str {
        "spammer"
    }

    fn decide(&mut self, obs: &Observation) -> Intent {
        if obs.distance > self.approach_range {
            return Intent::Move(Direction::Forward);
        }
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
    /// Stalemate-breaker: Spacer's own logic only ever attacks reactively
    /// (punishing a caught recovery window), so against an opponent that
    /// never attacks either (Turtle, Dummy), it would otherwise hold
    /// position and bait forever without ever landing a hit.
    probe: ProbeTimer,
}

impl Spacer {
    pub fn new() -> Spacer {
        Spacer { hold_range: Fixed::from_int(155), band: Fixed::from_int(15), bait_forward: true, probe: ProbeTimer::new(60) }
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
        // Called unconditionally, every decide(), regardless of which
        // branch below ends up firing - the stalemate clock has to keep
        // running even while we're mid-reposition, or it never reaches
        // threshold during a long bait-and-circle stretch.
        let should_probe = self.probe.should_probe(obs);

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

        if should_probe && obs.distance <= self.hold_range {
            return Intent::Attack(AttackKind::Light);
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
        self.probe.reset();
    }
}
