use std::collections::VecDeque;

use crate::combat::{self, HitLog};
use crate::fighter::Fighter;
use crate::fixed::{Fixed, Vec2};
use crate::hash::{self, Fnv1a};
use crate::movement;
use crate::rng::Pcg32;
use crate::ruleset::Ruleset;
use crate::state_machine::{advance, apply_intent};
use crate::types::{Facing, FighterState, FighterView, Intent, Observation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Freeze { ticks_left: u32 },
    Fighting,
    Intermission { ticks_left: u32 },
    MatchOver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundOutcome {
    Ko { winner: u8 },
    Timeout { winner: Option<u8> }, // None == exact draw
    DoubleKo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchEndReason {
    Ko,
    Timeout,
    SuddenDeath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchResult {
    pub winner: Option<u8>,
    pub reason: MatchEndReason,
}

pub struct WarnLog {
    pub fighter_idx: usize,
    pub second_warning: bool,
}

#[derive(Default)]
pub struct TickReport {
    pub hits: Vec<HitLog>,
    pub warnings: Vec<WarnLog>,
    pub round_ended: Option<RoundOutcome>,
    pub match_ended: Option<MatchResult>,
    pub state_hash: u64,
    pub chain_hash: u64,
}

#[derive(Clone)]
struct ObsFrame {
    tick: u32,
    round: u8,
    ticks_remaining: u32,
    views: [FighterView; 2],
}

pub struct Sim {
    pub ruleset: Ruleset,
    rng: Pcg32,
    fighters: [Fighter; 2],
    phase: Phase,
    round: u8,
    round_wins: [u8; 2],
    round_tick: u32,
    global_tick: u32,
    sudden_death: bool,
    chain_hash: u64,
    obs_buffer: VecDeque<ObsFrame>,

    // Passivity tracking, reset each round.
    passive_ticks: u32,
    passive_min_distance: Fixed,
    passive_tracked_fighter: Option<usize>,
}

impl Sim {
    pub fn new(ruleset: Ruleset, seed: u64) -> Sim {
        let half_sep = ruleset.start_separation / Fixed::from_int(2);
        let center = ruleset.arena_width / Fixed::from_int(2);
        let f0 = Fighter::new(Vec2::new(center - half_sep, Fixed::ZERO), Facing::Right, ruleset.vitality, ruleset.guard_max);
        let f1 = Fighter::new(Vec2::new(center + half_sep, Fixed::ZERO), Facing::Left, ruleset.vitality, ruleset.guard_max);
        let opening_freeze = ruleset.opening_freeze_ticks;
        let round_ticks = ruleset.round_ticks;
        let mut sim = Sim {
            rng: Pcg32::new(seed, 1),
            fighters: [f0, f1],
            phase: Phase::Freeze { ticks_left: opening_freeze },
            round: 1,
            round_wins: [0, 0],
            round_tick: 0,
            global_tick: 0,
            sudden_death: false,
            chain_hash: 0,
            obs_buffer: VecDeque::new(),
            passive_ticks: 0,
            passive_min_distance: Fixed::ZERO,
            passive_tracked_fighter: None,
            ruleset,
        };
        // Seed the observation buffer with the pre-match state so an agent
        // polled before the first tick sees a valid (if stale) observation.
        let views = [sim.fighters[0].view(), sim.fighters[1].view()];
        sim.obs_buffer.push_back(ObsFrame { tick: 0, round: 1, ticks_remaining: round_ticks, views });
        sim
    }

    pub fn is_over(&self) -> bool {
        matches!(self.phase, Phase::MatchOver)
    }

    pub fn round(&self) -> u8 {
        self.round
    }

    pub fn fighter_view(&self, idx: usize) -> FighterView {
        self.fighters[idx].view()
    }

    fn distance(&self) -> Fixed {
        (self.fighters[0].position.x - self.fighters[1].position.x).abs()
    }

    /// Observation for `idx`, read from `observation_delay` ticks in the past.
    pub fn observation(&self, idx: usize) -> Observation {
        let frame = self.obs_buffer.front().expect("obs buffer is seeded in Sim::new and never emptied");
        let (me, opp) = if idx == 0 { (frame.views[0], frame.views[1]) } else { (frame.views[1], frame.views[0]) };
        let corner_dist = |pos: Fixed| pos.min(self.ruleset.arena_width - pos);
        Observation {
            tick: frame.tick,
            round: frame.round,
            ticks_remaining: frame.ticks_remaining,
            me,
            opponent: opp,
            distance: (me.position.x - opp.position.x).abs(),
            my_corner_distance: corner_dist(me.position.x),
            opponent_corner_distance: corner_dist(opp.position.x),
        }
    }

    fn round_ticks_remaining(&self) -> u32 {
        let limit = if self.sudden_death { self.ruleset.sudden_death_ticks } else { self.ruleset.round_ticks };
        limit.saturating_sub(self.round_tick)
    }

    /// Advance the simulation by exactly one tick. This is the only way to mutate state.
    pub fn tick(&mut self, intents: [Intent; 2]) -> TickReport {
        let mut report = TickReport::default();

        match self.phase {
            Phase::MatchOver => {
                report.state_hash = self.compute_state_hash();
                report.chain_hash = self.chain_hash;
                return report;
            }
            Phase::Freeze { ticks_left } => {
                self.push_hash(&mut report);
                let left = ticks_left.saturating_sub(1);
                if left == 0 {
                    self.phase = Phase::Fighting;
                    for f in self.fighters.iter_mut() {
                        f.enter_state(FighterState::Idle);
                    }
                } else {
                    self.phase = Phase::Freeze { ticks_left: left };
                }
                self.global_tick += 1;
                return report;
            }
            Phase::Intermission { ticks_left } => {
                self.push_hash(&mut report);
                let left = ticks_left.saturating_sub(1);
                self.phase = if left == 0 { self.begin_next_round() } else { Phase::Intermission { ticks_left: left } };
                self.global_tick += 1;
                return report;
            }
            Phase::Fighting => {}
        }

        // Phase 1: hitstop freeze.
        if self.fighters[0].hitstop > 0 || self.fighters[1].hitstop > 0 {
            for f in self.fighters.iter_mut() {
                if f.hitstop > 0 {
                    f.hitstop -= 1;
                }
            }
            self.round_tick += 1;
            self.global_tick += 1;
            self.push_hash(&mut report);
            return report;
        }

        let dist_before = self.distance();

        // Phase 2: apply intents (ignored where disallowed).
        apply_intent(&mut self.fighters[0], intents[0], &self.ruleset, dist_before);
        apply_intent(&mut self.fighters[1], intents[1], &self.ruleset, dist_before);
        if matches!(intents[0], Intent::Throw) && self.fighters[0].state == FighterState::ThrowAttempt && self.fighters[0].state_tick == 0 {
            self.fighters[0].last_throw_attempt_tick = Some(self.global_tick);
        }
        if matches!(intents[1], Intent::Throw) && self.fighters[1].state == FighterState::ThrowAttempt && self.fighters[1].state_tick == 0 {
            self.fighters[1].last_throw_attempt_tick = Some(self.global_tick);
        }

        // Phase 3: advance state machines.
        let ev0 = advance(&mut self.fighters[0], &self.ruleset);
        let ev1 = advance(&mut self.fighters[1], &self.ruleset);
        if ev0.combo_reset {
            self.fighters[0].combo_count = 0;
        }
        if ev1.combo_reset {
            self.fighters[1].combo_count = 0;
        }

        // Phase 4: movement.
        movement::integrate(&mut self.fighters, &self.ruleset);

        // Phase 5 & 6: collisions, damage.
        let hits = combat::resolve(&mut self.fighters, &self.ruleset, self.global_tick);
        combat::regen_guard(&mut self.fighters, &self.ruleset);
        report.hits = hits;

        // Passivity.
        self.tick_passivity(&mut report);

        self.round_tick += 1;

        // Phase 7: round-end conditions.
        if let Some(outcome) = self.check_round_end() {
            self.apply_round_outcome(outcome, &mut report);
        }

        self.global_tick += 1;

        // Phase 8: push observation + hash.
        self.push_hash(&mut report);

        report
    }

    fn tick_passivity(&mut self, report: &mut TickReport) {
        let v0 = self.fighters[0].vitality;
        let v1 = self.fighters[1].vitality;
        let leader = if v0 > v1 {
            Some(0usize)
        } else if v1 > v0 {
            Some(1usize)
        } else {
            None
        };

        let Some(leader) = leader else {
            self.passive_ticks = 0;
            self.passive_tracked_fighter = None;
            return;
        };

        let dist = self.distance();
        if self.passive_tracked_fighter != Some(leader) {
            self.passive_tracked_fighter = Some(leader);
            self.passive_ticks = 0;
            self.passive_min_distance = dist;
        }

        let landed_hit_this_tick = report.hits.iter().any(|h| h.attacker_idx == leader && !h.blocked);
        if dist < self.passive_min_distance || landed_hit_this_tick {
            self.passive_min_distance = self.passive_min_distance.min(dist);
            self.passive_ticks = 0;
            return;
        }

        self.passive_ticks += 1;
        if self.passive_ticks >= self.ruleset.passivity_warning_ticks {
            self.passive_ticks = 0;
            self.passive_min_distance = dist;
            self.fighters[leader].warnings_this_round += 1;
            let second = self.fighters[leader].warnings_this_round >= 2;
            if second {
                let penalty = (self.fighters[leader].vitality * self.ruleset.passivity_penalty_fraction_pct) / 100;
                self.fighters[leader].vitality = (self.fighters[leader].vitality - penalty).max(0);
            }
            report.warnings.push(WarnLog { fighter_idx: leader, second_warning: second });
        }
    }

    fn check_round_end(&self) -> Option<RoundOutcome> {
        let v0 = self.fighters[0].vitality;
        let v1 = self.fighters[1].vitality;
        if v0 <= 0 && v1 <= 0 {
            return Some(RoundOutcome::DoubleKo);
        }
        if v0 <= 0 {
            return Some(RoundOutcome::Ko { winner: 1 });
        }
        if v1 <= 0 {
            return Some(RoundOutcome::Ko { winner: 0 });
        }
        if self.round_ticks_remaining() == 0 {
            let max_v = if self.sudden_death { self.ruleset.sudden_death_vitality } else { self.ruleset.vitality };
            let pct0 = (v0 * 10000) / max_v;
            let pct1 = (v1 * 10000) / max_v;
            let winner = if pct0 > pct1 {
                Some(0)
            } else if pct1 > pct0 {
                Some(1)
            } else {
                None
            };
            return Some(RoundOutcome::Timeout { winner });
        }
        None
    }

    fn apply_round_outcome(&mut self, outcome: RoundOutcome, report: &mut TickReport) {
        report.round_ended = Some(outcome);

        if self.sudden_death {
            let winner = match outcome {
                RoundOutcome::Ko { winner } => Some(winner),
                RoundOutcome::DoubleKo => None,
                RoundOutcome::Timeout { winner } => winner.or_else(|| self.tiebreak_by_hits()),
            };
            self.phase = Phase::MatchOver;
            report.match_ended = Some(MatchResult { winner, reason: MatchEndReason::SuddenDeath });
            return;
        }

        match outcome {
            RoundOutcome::Ko { winner } => self.round_wins[winner as usize] += 1,
            RoundOutcome::Timeout { winner: Some(w) } => self.round_wins[w as usize] += 1,
            RoundOutcome::Timeout { winner: None } | RoundOutcome::DoubleKo => {}
        }

        if self.round_wins[0] >= self.ruleset.rounds_to_win {
            self.phase = Phase::MatchOver;
            let reason = if matches!(outcome, RoundOutcome::Ko { .. }) { MatchEndReason::Ko } else { MatchEndReason::Timeout };
            report.match_ended = Some(MatchResult { winner: Some(0), reason });
            return;
        }
        if self.round_wins[1] >= self.ruleset.rounds_to_win {
            self.phase = Phase::MatchOver;
            let reason = if matches!(outcome, RoundOutcome::Ko { .. }) { MatchEndReason::Ko } else { MatchEndReason::Timeout };
            report.match_ended = Some(MatchResult { winner: Some(1), reason });
            return;
        }

        if self.round >= self.ruleset.max_rounds {
            // No one reached rounds_to_win after max_rounds: sudden death.
            self.sudden_death = true;
            self.round += 1;
            self.round_wins = [0, 0];
            self.phase = Phase::Intermission { ticks_left: self.ruleset.intermission_ticks };
            return;
        }

        self.round += 1;
        self.phase = Phase::Intermission { ticks_left: self.ruleset.intermission_ticks };
    }

    fn tiebreak_by_hits(&self) -> Option<u8> {
        let h0 = self.fighters[0].total_hits_landed;
        let h1 = self.fighters[1].total_hits_landed;
        if h0 > h1 {
            Some(0)
        } else if h1 > h0 {
            Some(1)
        } else {
            None
        }
    }

    fn begin_next_round(&mut self) -> Phase {
        let half_sep = self.ruleset.start_separation / Fixed::from_int(2);
        let center = self.ruleset.arena_width / Fixed::from_int(2);
        let vitality = if self.sudden_death { self.ruleset.sudden_death_vitality } else { self.ruleset.vitality };
        for (i, f) in self.fighters.iter_mut().enumerate() {
            let x = if i == 0 { center - half_sep } else { center + half_sep };
            *f = Fighter::new(Vec2::new(x, Fixed::ZERO), if i == 0 { Facing::Right } else { Facing::Left }, vitality, self.ruleset.guard_max);
        }
        self.round_tick = 0;
        self.passive_ticks = 0;
        self.passive_tracked_fighter = None;
        Phase::Freeze { ticks_left: self.ruleset.opening_freeze_ticks }
    }

    fn compute_state_hash(&self) -> u64 {
        let mut h = Fnv1a::new();
        h.write_u32(self.global_tick);
        h.write_u32(self.round_tick);
        h.write_i32(self.round as i32);
        h.write_i32(self.round_wins[0] as i32);
        h.write_i32(self.round_wins[1] as i32);
        h.write_i32(self.sudden_death as i32);
        match self.phase {
            Phase::Freeze { ticks_left } => {
                h.write_u8(0);
                h.write_u32(ticks_left);
            }
            Phase::Fighting => h.write_u8(1),
            Phase::Intermission { ticks_left } => {
                h.write_u8(2);
                h.write_u32(ticks_left);
            }
            Phase::MatchOver => h.write_u8(3),
        }
        for f in &self.fighters {
            h.write_i32(f.position.x.raw());
            h.write_i32(f.position.y.raw());
            h.write_i32(f.velocity.x.raw());
            h.write_i32(f.velocity.y.raw());
            h.write_u8(f.facing.is_right() as u8);
            h.write_i32(f.vitality);
            h.write_i32(f.surge);
            h.write_i32(f.guard);
            h.write_u8(f.state as u8);
            h.write_i32(f.state_tick as i32);
            h.write_i32(f.combo_count as i32);
            h.write_i32(f.hitstop as i32);
            h.write_i32(f.warnings_this_round as i32);
            h.write_i32(f.total_hits_landed as i32);
            h.write_i32(f.total_damage_dealt);
        }
        h.finish()
    }

    fn push_hash(&mut self, report: &mut TickReport) {
        let views = [self.fighters[0].view(), self.fighters[1].view()];
        self.obs_buffer.push_back(ObsFrame {
            tick: self.global_tick,
            round: self.round,
            ticks_remaining: self.round_ticks_remaining(),
            views,
        });
        while self.obs_buffer.len() > (self.ruleset.observation_delay as usize + 1) {
            self.obs_buffer.pop_front();
        }
        let state_hash = self.compute_state_hash();
        self.chain_hash = hash::chain(self.chain_hash, state_hash);
        report.state_hash = state_hash;
        report.chain_hash = self.chain_hash;
    }

    pub fn chain_hash(&self) -> u64 {
        self.chain_hash
    }

    pub fn global_tick(&self) -> u32 {
        self.global_tick
    }

    /// Draws entropy from the simulation's owned PRNG. Nothing in v0 calls
    /// this yet, but the plumbing must exist.
    pub fn roll(&mut self, bound: u32) -> u32 {
        self.rng.next_bounded(bound)
    }
}
