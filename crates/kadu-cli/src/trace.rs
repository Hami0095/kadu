//! Generates a rich, per-tick JSON trace for the browser viewer
//! (`viewer/index.html`). The viewer reads this file directly and does not
//! re-simulate anything - re-simulation in the browser is the right
//! long-term design (WASM, v0.4) but hardcoding it now would cost a week
//! for no v0.3 benefit. This module runs the simulation once, host-side,
//! in Rust - fully deterministic - and writes out everything the viewer
//! needs to render every tick without ever calling back into kadu-core.
//!
//! Positions are converted from fixed-point to f64 here. That's fine:
//! determinism only has to hold for the *simulation*, not for how an
//! already-computed result is displayed. Nothing in this module feeds back
//! into the sim.

use kadu_agent::Agent;
use kadu_core::ruleset::Ruleset;
use kadu_core::{AttackKind, Direction, Intent, MatchEndReason, RoundOutcome};
use kadu_replay::{Recorder, Replay};
use serde::Serialize;

fn f(v: kadu_core::Fixed) -> f64 {
    v.raw() as f64 / 65536.0
}

#[derive(Serialize)]
pub struct RectDef {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

fn rect(r: kadu_core::geometry::Rect) -> RectDef {
    RectDef { x: f(r.x), y: f(r.y), w: f(r.w), h: f(r.h) }
}

#[derive(Serialize)]
pub struct HitboxDefs {
    pub light: RectDef,
    pub medium: RectDef,
    pub heavy: RectDef,
    pub throw: RectDef,
    pub light_crouch: RectDef,
    pub medium_crouch: RectDef,
}

#[derive(Serialize)]
pub struct HurtboxDefs {
    pub head: RectDef,
    pub torso: RectDef,
    pub arm_left: RectDef,
    pub arm_right: RectDef,
    pub leg_left: RectDef,
    pub leg_right: RectDef,
    pub head_crouch: RectDef,
    pub torso_crouch: RectDef,
    pub arm_left_crouch: RectDef,
    pub arm_right_crouch: RectDef,
    pub leg_left_crouch: RectDef,
    pub leg_right_crouch: RectDef,
}

#[derive(Serialize)]
pub struct TraceMeta {
    pub engine_version: String,
    pub ruleset_version: String,
    pub ruleset_hash: String,
    pub seed: u64,
    pub agent_names: [String; 2],
    pub tick_rate: i32,
    pub arena_width: f64,
    pub ceiling: f64,
    pub fighter_width: f64,
    pub fighter_height: f64,
    pub rounds_to_win: u8,
}

#[derive(Serialize, Clone)]
pub struct TraceFighter {
    pub x: f64,
    pub y: f64,
    pub facing: &'static str,
    pub vitality: i32,
    pub surge: i32,
    pub guard: i32,
    pub state: String,
    pub state_tick: u16,
    pub combo_count: u8,
    pub airborne: bool,
    /// Which hitbox key (matching `HitboxDefs`' field names) is active
    /// this tick, if any, so the viewer can draw it without reimplementing
    /// any move-selection logic.
    pub active_hitbox: Option<String>,
    /// True while a throw's active window is open - drawn using the
    /// `throw` hitbox def, kept separate from `active_hitbox` since a
    /// throw isn't an `AttackKind`.
    pub throw_active: bool,
}

fn intent_json(intent: Intent) -> serde_json::Value {
    use serde_json::json;
    fn dir(d: Direction) -> &'static str {
        match d {
            Direction::Neutral => "neutral",
            Direction::Forward => "forward",
            Direction::Back => "back",
            Direction::Up => "up",
            Direction::Down => "down",
            Direction::UpForward => "up_forward",
            Direction::UpBack => "up_back",
            Direction::DownForward => "down_forward",
            Direction::DownBack => "down_back",
        }
    }
    fn kind(k: AttackKind) -> &'static str {
        match k {
            AttackKind::Light => "light",
            AttackKind::Medium => "medium",
            AttackKind::Heavy => "heavy",
        }
    }
    match intent {
        Intent::None => json!({"action": "none"}),
        Intent::Move(d) => json!({"action": "move", "direction": dir(d)}),
        Intent::Attack(k) => json!({"action": "attack", "attack": kind(k)}),
        Intent::Block => json!({"action": "block"}),
        Intent::Crouch => json!({"action": "crouch"}),
        Intent::Dash(d) => json!({"action": "dash", "direction": dir(d)}),
        Intent::Jump(d) => json!({"action": "jump", "direction": dir(d)}),
        Intent::Throw => json!({"action": "throw"}),
    }
}

#[derive(Serialize)]
pub struct TraceHit {
    pub attacker: usize,
    pub defender: usize,
    pub blocked: bool,
    pub damage: i32,
    pub kind: Option<String>,
    pub is_throw: bool,
    pub is_tech: bool,
    pub hard_knockdown: bool,
}

#[derive(Serialize)]
pub struct TraceWarning {
    pub fighter_idx: usize,
    pub second_warning: bool,
}

#[derive(Serialize)]
pub struct TraceTick {
    pub tick: u32,
    pub round: u8,
    pub ticks_remaining: u32,
    pub fighters: [TraceFighter; 2],
    pub intents: [serde_json::Value; 2],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hits: Vec<TraceHit>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<TraceWarning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub round_ended: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_winner: Option<Option<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_reason: Option<String>,
}

#[derive(Serialize)]
pub struct Trace {
    pub meta: TraceMeta,
    pub hitboxes: HitboxDefs,
    pub hurtboxes: HurtboxDefs,
    pub ticks: Vec<TraceTick>,
}

fn hitbox_key(kind: AttackKind, crouching: bool) -> &'static str {
    match (kind, crouching) {
        (AttackKind::Light, true) => "light_crouch",
        (AttackKind::Medium, true) => "medium_crouch",
        (AttackKind::Light, false) => "light",
        (AttackKind::Medium, false) => "medium",
        (AttackKind::Heavy, _) => "heavy",
    }
}

/// Runs a full match once and returns both the verify-able `Replay` (same
/// as `run_match`) and a rich per-tick `Trace` for the viewer. One
/// simulation pass produces both - the trace is a superset of what the
/// replay needs, not a second run.
pub fn run_match_with_trace(
    mut agent_a: Box<dyn Agent>,
    mut agent_b: Box<dyn Agent>,
    ruleset: Ruleset,
    seed: u64,
    ruleset_toml: String,
    agent_a_name: &str,
    agent_b_name: &str,
) -> (Replay, Trace) {
    let reflex_interval = ruleset.reflex_interval;
    let meta = TraceMeta {
        engine_version: kadu_replay::ENGINE_VERSION.to_string(),
        ruleset_version: ruleset.version.clone(),
        ruleset_hash: format!("{:#018x}", ruleset.content_hash()),
        seed,
        agent_names: [agent_a_name.to_string(), agent_b_name.to_string()],
        tick_rate: ruleset.tick_rate,
        arena_width: f(ruleset.arena_width),
        ceiling: f(ruleset.ceiling),
        fighter_width: f(ruleset.fighter_width),
        fighter_height: f(ruleset.fighter_height),
        rounds_to_win: ruleset.rounds_to_win,
    };
    let hitboxes = HitboxDefs {
        light: rect(ruleset.light_hitbox),
        medium: rect(ruleset.medium_hitbox),
        heavy: rect(ruleset.heavy_hitbox),
        throw: rect(ruleset.throw_hitbox),
        light_crouch: rect(ruleset.light_hitbox_crouch),
        medium_crouch: rect(ruleset.medium_hitbox_crouch),
    };
    let hurtboxes = HurtboxDefs {
        head: rect(ruleset.hurt_head),
        torso: rect(ruleset.hurt_torso),
        arm_left: rect(ruleset.hurt_arm_left),
        arm_right: rect(ruleset.hurt_arm_right),
        leg_left: rect(ruleset.hurt_leg_left),
        leg_right: rect(ruleset.hurt_leg_right),
        head_crouch: rect(ruleset.hurt_head_crouch),
        torso_crouch: rect(ruleset.hurt_torso_crouch),
        arm_left_crouch: rect(ruleset.hurt_arm_left_crouch),
        arm_right_crouch: rect(ruleset.hurt_arm_right_crouch),
        leg_left_crouch: rect(ruleset.hurt_leg_left_crouch),
        leg_right_crouch: rect(ruleset.hurt_leg_right_crouch),
    };

    let mut rec = Recorder::new(ruleset, ruleset_toml, seed, agent_a_name, agent_b_name);
    let mut last_intents = [Intent::None, Intent::None];
    let mut ticks: Vec<TraceTick> = Vec::new();

    loop {
        let tick = rec.sim.global_tick();
        if tick % reflex_interval == 0 {
            let obs_a = rec.sim.observation(0);
            let obs_b = rec.sim.observation(1);
            last_intents = [agent_a.decide(&obs_a), agent_b.decide(&obs_b)];
        }
        let report = rec.tick(last_intents);

        let views = [rec.sim.fighter_view(0), rec.sim.fighter_view(1)];
        let mut trace_fighters: Vec<TraceFighter> = Vec::with_capacity(2);
        for i in 0..2 {
            let v = &views[i];
            let variant = rec.sim.fighter_attack_variant(i);
            let active_hitbox = variant.filter(|_| v.state == kadu_core::FighterState::AttackActive).map(|(k, crouching, _)| hitbox_key(k, crouching).to_string());
            trace_fighters.push(TraceFighter {
                x: f(v.position.x),
                y: f(v.position.y),
                facing: if v.facing == kadu_core::Facing::Right { "right" } else { "left" },
                vitality: v.vitality,
                surge: v.surge,
                guard: v.guard,
                state: format!("{:?}", v.state),
                state_tick: v.state_tick,
                combo_count: v.combo_count,
                airborne: v.airborne,
                active_hitbox,
                throw_active: rec.sim.fighter_throw_active(i),
            });
        }

        let hits = report
            .hits
            .iter()
            .map(|h| TraceHit {
                attacker: h.attacker_idx,
                defender: h.defender_idx,
                blocked: h.blocked,
                damage: h.damage,
                kind: h.kind.map(|k| hitbox_key(k, false).trim_end_matches("_crouch").to_string()),
                is_throw: h.is_throw,
                is_tech: h.is_tech,
                hard_knockdown: h.hard_knockdown,
            })
            .collect();

        let warnings = report.warnings.iter().map(|w| TraceWarning { fighter_idx: w.fighter_idx, second_warning: w.second_warning }).collect();

        let round_ended = report.round_ended.map(|o| match o {
            RoundOutcome::Ko { .. } => "ko".to_string(),
            RoundOutcome::Timeout { .. } => "timeout".to_string(),
            RoundOutcome::DoubleKo => "double_ko".to_string(),
        });

        let (match_winner, match_reason) = match report.match_ended {
            Some(m) => (Some(m.winner), Some(match m.reason {
                MatchEndReason::Ko => "ko".to_string(),
                MatchEndReason::Timeout => "timeout".to_string(),
                MatchEndReason::SuddenDeath => "sudden_death".to_string(),
            })),
            None => (None, None),
        };

        ticks.push(TraceTick {
            tick: rec.sim.global_tick(),
            round: rec.sim.round(),
            ticks_remaining: rec.sim.round_ticks_remaining(),
            fighters: [trace_fighters[0].clone(), trace_fighters[1].clone()],
            intents: [intent_json(last_intents[0]), intent_json(last_intents[1])],
            hits,
            warnings,
            round_ended,
            match_winner,
            match_reason,
        });

        let is_over = rec.is_over();
        if is_over {
            break;
        }
        if rec.sim.global_tick() > 2_000_000 {
            eprintln!("match exceeded safety tick limit, aborting trace generation");
            break;
        }
    }

    let trace = Trace { meta, hitboxes, hurtboxes, ticks };
    let replay = rec.into_replay();
    (replay, trace)
}
