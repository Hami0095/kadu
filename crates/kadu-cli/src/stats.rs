//! Match statistics: "you cannot tune a game you cannot measure." Collected
//! by an external tick-by-tick observer (Sim's public TickReport, FighterView
//! and the host-only fighter_attack_kind query), not by instrumenting
//! kadu-core's hashed state, so this carries zero risk to the determinism
//! aggregate and zero cost to `kadu bench`'s hot loop - `run_match` (bench's
//! path) is untouched; only `run_match_with_stats` (used by `kadu run` and
//! `kadu tourney`) pays for any of this.

use std::collections::BTreeMap;

use kadu_agent::Agent;
use kadu_core::{AttackKind, FighterState, Intent, Ruleset};
use kadu_replay::{Recorder, Replay};
use serde::Serialize;

const STATE_NAMES: [&str; 24] = [
    "Idle",
    "WalkFwd",
    "WalkBack",
    "Crouch",
    "DashFwd",
    "DashBack",
    "JumpRise",
    "JumpFall",
    "AttackStartup",
    "AttackActive",
    "AttackRecovery",
    "BlockStand",
    "BlockCrouch",
    "BlockStun",
    "HitStun",
    "Knockdown",
    "WakeUp",
    "GuardCrush",
    "Dizzy",
    "ThrowAttempt",
    "ThrowWhiff",
    "Thrown",
    "RoundFreeze",
    "RoundOver",
];

pub const DISTANCE_BUCKETS: usize = 8;

#[derive(Default, Clone, Serialize)]
pub struct AttackTypeCounts {
    pub attempted: u32,
    pub landed: u32,
    pub blocked: u32,
    pub whiffed: u32,
}

#[derive(Default, Clone, Serialize)]
pub struct AttackCounts {
    pub light: AttackTypeCounts,
    pub medium: AttackTypeCounts,
    pub heavy: AttackTypeCounts,
}

impl AttackCounts {
    fn of_mut(&mut self, kind: AttackKind) -> &mut AttackTypeCounts {
        match kind {
            AttackKind::Light => &mut self.light,
            AttackKind::Medium => &mut self.medium,
            AttackKind::Heavy => &mut self.heavy,
        }
    }

    fn finalize_whiffs(&mut self) {
        for c in [&mut self.light, &mut self.medium, &mut self.heavy] {
            c.whiffed = c.attempted.saturating_sub(c.landed + c.blocked);
        }
    }
}

#[derive(Default, Clone, Serialize)]
pub struct DamageStats {
    pub dealt: i64,
    pub taken: i64,
    pub chip_dealt: i64,
    /// Base move damage minus actual damage dealt, for every unblocked
    /// strike - an approximate but honest single number combining combo
    /// scaling, crouch scaling, and kill-clamping.
    pub scaled_away: i64,
}

#[derive(Default, Clone, Serialize)]
pub struct ComboStats {
    pub count: u32,
    pub longest: u8,
    pub average_length: f64,
}

#[derive(Default, Clone, Serialize)]
pub struct GuardStats {
    pub damage_taken: i64,
    pub crushes: u32,
    pub ticks_blocking: u32,
}

#[derive(Default, Clone, Serialize)]
pub struct SurgeStats {
    pub gained: i64,
    pub peak: i32,
    pub ticks_at_max: u32,
}

#[derive(Default, Clone, Serialize)]
pub struct ThrowStats {
    pub attempted: u32,
    pub landed: u32,
    pub teched: u32,
}

#[derive(Default, Clone, Serialize)]
pub struct PassivityStats {
    pub warnings: u32,
    pub penalties: u32,
}

#[derive(Default, Clone, Serialize)]
pub struct FighterStats {
    pub attacks: AttackCounts,
    pub damage: DamageStats,
    pub combos: ComboStats,
    pub guard: GuardStats,
    pub surge: SurgeStats,
    pub throws: ThrowStats,
    pub passivity: PassivityStats,
    /// Ticks spent in each FighterState, by name (states never entered are
    /// omitted).
    pub state_ticks: BTreeMap<String, u32>,
    pub state_ticks_pct: BTreeMap<String, f64>,

    #[serde(skip)]
    state_tick_counts: [u32; 24],
    #[serde(skip)]
    total_ticks: u32,
    #[serde(skip)]
    combo_length_sum: u64,
}

impl FighterStats {
    fn finalize(&mut self) {
        self.attacks.finalize_whiffs();
        for (i, &name) in STATE_NAMES.iter().enumerate() {
            let ticks = self.state_tick_counts[i];
            if ticks > 0 {
                self.state_ticks.insert(name.to_string(), ticks);
                let pct = if self.total_ticks > 0 { ticks as f64 * 100.0 / self.total_ticks as f64 } else { 0.0 };
                self.state_ticks_pct.insert(name.to_string(), pct);
            }
        }
        self.combos.average_length = if self.combos.count > 0 { self.combo_length_sum as f64 / self.combos.count as f64 } else { 0.0 };
    }
}

fn merge_into(total: &mut FighterStats, round: &FighterStats) {
    for (t, r) in [
        (&mut total.attacks.light, &round.attacks.light),
        (&mut total.attacks.medium, &round.attacks.medium),
        (&mut total.attacks.heavy, &round.attacks.heavy),
    ] {
        t.attempted += r.attempted;
        t.landed += r.landed;
        t.blocked += r.blocked;
        t.whiffed += r.whiffed;
    }
    total.damage.dealt += round.damage.dealt;
    total.damage.taken += round.damage.taken;
    total.damage.chip_dealt += round.damage.chip_dealt;
    total.damage.scaled_away += round.damage.scaled_away;
    total.combos.count += round.combos.count;
    total.combos.longest = total.combos.longest.max(round.combos.longest);
    total.combo_length_sum += round.combo_length_sum;
    total.guard.damage_taken += round.guard.damage_taken;
    total.guard.crushes += round.guard.crushes;
    total.guard.ticks_blocking += round.guard.ticks_blocking;
    total.surge.gained += round.surge.gained;
    total.surge.peak = total.surge.peak.max(round.surge.peak);
    total.surge.ticks_at_max += round.surge.ticks_at_max;
    total.throws.attempted += round.throws.attempted;
    total.throws.landed += round.throws.landed;
    total.throws.teched += round.throws.teched;
    total.passivity.warnings += round.passivity.warnings;
    total.passivity.penalties += round.passivity.penalties;
    for i in 0..24 {
        total.state_tick_counts[i] += round.state_tick_counts[i];
    }
    total.total_ticks += round.total_ticks;
}

#[derive(Default, Clone, Serialize)]
pub struct RoundStats {
    pub round: u8,
    pub ending: String,
    pub duration_ticks: u32,
    pub distance_histogram: [u32; DISTANCE_BUCKETS],
    pub trades: u32,
    pub fighters: [FighterStats; 2],
}

#[derive(Default, Clone, Serialize)]
pub struct MatchStats {
    pub rounds: Vec<RoundStats>,
    pub total_trades: u32,
    pub totals: [FighterStats; 2],
}

/// Per-fighter state the collector carries across ticks to detect edges
/// (state transitions, value deltas) that a single tick's report doesn't
/// carry on its own.
#[derive(Default, Clone, Copy)]
struct Watch {
    prev_state: Option<FighterState>,
    prev_guard: i32,
    prev_surge: i32,
    prev_combo_count: u8,
    combo_attacker: Option<usize>,
}

fn base_damage(rs: &Ruleset, kind: AttackKind) -> i32 {
    match kind {
        AttackKind::Light => rs.light.damage,
        AttackKind::Medium => rs.medium.damage,
        AttackKind::Heavy => rs.heavy.damage,
    }
}

/// Runs a full match, exactly like `run_match`, but also collects the full
/// stats block. Only used by `kadu run` and `kadu tourney` - `kadu bench`'s
/// hot loop stays on the plain, stats-free path.
pub fn run_match_with_stats(
    mut agent_a: Box<dyn Agent>,
    mut agent_b: Box<dyn Agent>,
    ruleset: Ruleset,
    seed: u64,
    ruleset_toml: String,
    agent_a_name: &str,
    agent_b_name: &str,
) -> (Replay, MatchStats) {
    let reflex_interval = ruleset.reflex_interval;
    let surge_max = ruleset.surge_max;
    let arena_width = ruleset.arena_width.to_int().max(1);
    let rs_for_damage = ruleset.clone();

    let mut rec = Recorder::new(ruleset, ruleset_toml, seed, agent_a_name, agent_b_name);
    let mut last_intents = [Intent::None, Intent::None];

    let mut match_stats = MatchStats::default();
    let mut cur = RoundStats { round: 1, ..Default::default() };
    let mut watch = [Watch::default(), Watch::default()];

    let flush_round = |cur: &mut RoundStats, match_stats: &mut MatchStats| {
        cur.duration_ticks = cur.fighters[0].total_ticks.max(cur.fighters[1].total_ticks);
        cur.fighters[0].finalize();
        cur.fighters[1].finalize();
        merge_into(&mut match_stats.totals[0], &cur.fighters[0]);
        merge_into(&mut match_stats.totals[1], &cur.fighters[1]);
        match_stats.total_trades += cur.trades;
        match_stats.rounds.push(cur.clone());
    };

    loop {
        let tick = rec.sim.global_tick();
        if tick % reflex_interval == 0 {
            let obs_a = rec.sim.observation(0);
            let obs_b = rec.sim.observation(1);
            last_intents = [agent_a.decide(&obs_a), agent_b.decide(&obs_b)];
        }
        let report = rec.tick(last_intents);
        let views = [rec.sim.fighter_view(0), rec.sim.fighter_view(1)];
        let attack_kinds = [rec.sim.fighter_attack_kind(0), rec.sim.fighter_attack_kind(1)];
        // sim.round() may already reflect the *next* round by the time we
        // read it here (Sim advances it internally the same tick a round
        // ends), so this tick's data - including the ending label below -
        // is deliberately attributed to `cur` (the round that was current
        // going into this tick) before any flush happens.

        if views[0].state != FighterState::RoundFreeze {
            for i in 0..2 {
                let v = &views[i];
                let w = &mut watch[i];

                cur.fighters[i].total_ticks += 1;
                cur.fighters[i].state_tick_counts[v.state as usize] += 1;
                if matches!(v.state, FighterState::BlockStand | FighterState::BlockCrouch | FighterState::BlockStun) {
                    cur.fighters[i].guard.ticks_blocking += 1;
                }
                cur.fighters[i].surge.peak = cur.fighters[i].surge.peak.max(v.surge);
                if v.surge >= surge_max {
                    cur.fighters[i].surge.ticks_at_max += 1;
                }

                // Attempt detection: just entered *Startup this tick.
                if v.state == FighterState::AttackStartup && w.prev_state != Some(FighterState::AttackStartup) {
                    if let Some(kind) = attack_kinds[i] {
                        cur.fighters[i].attacks.of_mut(kind).attempted += 1;
                    }
                }
                if v.state == FighterState::ThrowAttempt && w.prev_state != Some(FighterState::ThrowAttempt) {
                    cur.fighters[i].throws.attempted += 1;
                }

                // Guard/surge deltas.
                let guard_delta = w.prev_guard - v.guard;
                if guard_delta > 0 {
                    cur.fighters[i].guard.damage_taken += guard_delta as i64;
                }
                if v.state == FighterState::GuardCrush && w.prev_state != Some(FighterState::GuardCrush) {
                    cur.fighters[i].guard.crushes += 1;
                }
                let surge_delta = v.surge - w.prev_surge;
                if surge_delta > 0 {
                    cur.fighters[i].surge.gained += surge_delta as i64;
                }

                // Combo end: this fighter (as defender) just dropped back to 0.
                if v.combo_count == 0 && w.prev_combo_count > 0 {
                    if let Some(attacker) = w.combo_attacker.take() {
                        let len = w.prev_combo_count;
                        cur.fighters[attacker].combos.count += 1;
                        cur.fighters[attacker].combos.longest = cur.fighters[attacker].combos.longest.max(len);
                        cur.fighters[attacker].combo_length_sum += len as u64;
                    }
                }

                w.prev_guard = v.guard;
                w.prev_surge = v.surge;
                w.prev_combo_count = v.combo_count;
            }

            let dist = (views[0].position.x - views[1].position.x).abs().to_int().unsigned_abs();
            let bucket = ((dist as usize) * DISTANCE_BUCKETS / (arena_width as usize + 1)).min(DISTANCE_BUCKETS - 1);
            cur.distance_histogram[bucket] += 1;

            let mut hit_this_tick = [false, false];
            for h in &report.hits {
                if h.is_tech {
                    cur.fighters[h.attacker_idx].throws.teched += 1;
                    continue;
                }
                hit_this_tick[h.attacker_idx] = true;
                let (attacker, defender) = (h.attacker_idx, h.defender_idx);

                if h.is_throw {
                    cur.fighters[attacker].throws.landed += 1;
                    cur.fighters[attacker].damage.dealt += h.damage as i64;
                    cur.fighters[defender].damage.taken += h.damage as i64;
                    continue;
                }

                let kind = h.kind.expect("non-throw, non-tech hits always carry a kind");
                let counts = cur.fighters[attacker].attacks.of_mut(kind);
                if h.blocked {
                    counts.blocked += 1;
                    cur.fighters[attacker].damage.chip_dealt += h.damage as i64;
                } else {
                    counts.landed += 1;
                    cur.fighters[attacker].damage.dealt += h.damage as i64;
                    cur.fighters[defender].damage.taken += h.damage as i64;
                    let scaled_away = (base_damage(&rs_for_damage, kind) - h.damage).max(0);
                    cur.fighters[attacker].damage.scaled_away += scaled_away as i64;

                    if views[defender].combo_count == 1 {
                        watch[defender].combo_attacker = Some(attacker);
                    }
                }
            }
            if hit_this_tick[0] && hit_this_tick[1] {
                cur.trades += 1;
            }

            for w in &report.warnings {
                cur.fighters[w.fighter_idx].passivity.warnings += 1;
                if w.second_warning {
                    cur.fighters[w.fighter_idx].passivity.penalties += 1;
                }
            }
        }

        watch[0].prev_state = Some(views[0].state);
        watch[1].prev_state = Some(views[1].state);

        if let Some(outcome) = report.round_ended {
            cur.ending = if report.match_ended.map(|m| m.reason == kadu_core::MatchEndReason::SuddenDeath).unwrap_or(false) {
                "sudden_death".to_string()
            } else {
                match outcome {
                    kadu_core::RoundOutcome::Ko { .. } => "ko".to_string(),
                    kadu_core::RoundOutcome::Timeout { .. } => "timeout".to_string(),
                    kadu_core::RoundOutcome::DoubleKo => "double_ko".to_string(),
                }
            };

            let next_round = rec.sim.round();
            flush_round(&mut cur, &mut match_stats);
            cur = RoundStats { round: next_round, ..Default::default() };
            watch = [Watch::default(), Watch::default()];
        }

        if report.match_ended.is_some() {
            break;
        }
        if rec.sim.global_tick() > 2_000_000 {
            eprintln!("match exceeded safety tick limit, aborting stats collection");
            break;
        }
    }

    // A round_ended event already flushes `cur` in-loop (every match_ended
    // coincides with one), leaving a fresh empty `cur` behind. Only flush
    // again here if the loop broke some other way (the safety-limit abort)
    // and left real unflushed data.
    if cur.fighters[0].total_ticks > 0 || cur.fighters[1].total_ticks > 0 {
        flush_round(&mut cur, &mut match_stats);
    }
    match_stats.totals[0].finalize();
    match_stats.totals[1].finalize();

    let replay = rec.into_replay();
    (replay, match_stats)
}
