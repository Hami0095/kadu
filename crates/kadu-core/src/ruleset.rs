//! Loads every tunable constant from a ruleset TOML file. Nothing here is
//! hardcoded into the simulation; changing the file changes behaviour with
//! no recompilation.

use crate::fixed::Fixed;
use crate::geometry::Rect;
use crate::hash::Fnv1a;
use serde::Deserialize;
use std::fmt;

#[derive(Debug, Deserialize, Clone, Default)]
struct RawMeta {
    #[serde(default)]
    version: String,
}

#[derive(Debug, Deserialize, Clone)]
struct RawTiming {
    tick_rate: i32,
    round_ticks: i32,
    intermission_ticks: i32,
    opening_freeze_ticks: i32,
    rounds_to_win: i32,
    max_rounds: i32,
}

#[derive(Debug, Deserialize, Clone)]
struct RawArena {
    arena_width: i32,
    ceiling: i32,
    fighter_height: i32,
    fighter_width: i32,
    start_separation: i32,
    #[serde(default)]
    start_separation_jitter: i32,
}

#[derive(Debug, Deserialize, Clone)]
struct RawMeters {
    vitality: i32,
    surge_max: i32,
    guard_max: i32,
}

#[derive(Debug, Deserialize, Clone)]
struct RawAgent {
    observation_delay: i32,
    reflex_interval: i32,
}

#[derive(Debug, Deserialize, Clone)]
struct RawMovement {
    gravity: i32,
    walk_speed: i32,
    dash_speed: i32,
    dash_ticks: i32,
    jump_velocity: i32,
}

#[derive(Debug, Deserialize, Clone)]
struct RawSuddenDeath {
    ticks: i32,
    vitality: i32,
}

#[derive(Debug, Deserialize, Clone)]
struct RawPassivity {
    warning_ticks: i32,
    penalty_fraction_pct: i32,
}

#[derive(Debug, Deserialize, Clone)]
struct RawGuardCrush {
    crush_ticks: i32,
    refill_to: i32,
}

#[derive(Debug, Deserialize, Clone)]
struct RawThrow {
    tech_window_ticks: i32,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct RawFrameData {
    #[serde(default)]
    block_advantage_whitelist: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct RawMove {
    startup: i32,
    active: i32,
    recovery: i32,
    damage: i32,
    #[serde(default)]
    block_damage: i32,
    #[serde(default)]
    hitstun: i32,
    #[serde(default)]
    blockstun: i32,
    #[serde(default)]
    guard_damage: i32,
    hitstop: i32,
}

#[derive(Debug, Deserialize, Clone)]
struct RawMoves {
    light: RawMove,
    medium: RawMove,
    heavy: RawMove,
    throw: RawMove,
}

#[derive(Debug, Deserialize, Clone)]
struct RawCombo {
    scaling_pct: Vec<i32>,
    floor_pct: i32,
    hard_knockdown_hit: i32,
}

#[derive(Debug, Deserialize, Clone)]
struct RawHitboxes {
    light_hitbox: [i32; 4],
    medium_hitbox: [i32; 4],
    heavy_hitbox: [i32; 4],
    throw_hitbox: [i32; 4],
    light_hitbox_crouch: [i32; 4],
    medium_hitbox_crouch: [i32; 4],
    hurt_head: [i32; 4],
    hurt_torso: [i32; 4],
    hurt_arm_left: [i32; 4],
    hurt_arm_right: [i32; 4],
    hurt_leg_left: [i32; 4],
    hurt_leg_right: [i32; 4],
    hurt_head_crouch: [i32; 4],
    hurt_torso_crouch: [i32; 4],
    hurt_arm_left_crouch: [i32; 4],
    hurt_arm_right_crouch: [i32; 4],
    hurt_leg_left_crouch: [i32; 4],
    hurt_leg_right_crouch: [i32; 4],
}

#[derive(Debug, Deserialize, Clone)]
struct RawRuleset {
    timing: RawTiming,
    arena: RawArena,
    meters: RawMeters,
    agent: RawAgent,
    movement: RawMovement,
    sudden_death: RawSuddenDeath,
    passivity: RawPassivity,
    guard_crush: RawGuardCrush,
    throw: RawThrow,
    moves: RawMoves,
    combo: RawCombo,
    hitboxes: RawHitboxes,
    #[serde(default)]
    frame_data: RawFrameData,
    #[serde(default)]
    meta: RawMeta,
}

#[derive(Debug, Clone)]
pub struct MoveSpec {
    pub startup: u16,
    pub active: u16,
    pub recovery: u16,
    pub damage: i32,
    pub block_damage: i32,
    pub hitstun: u16,
    pub blockstun: u16,
    pub guard_damage: i32,
    pub hitstop: u16,
}

impl MoveSpec {
    pub const fn total_frames(&self) -> u16 {
        self.startup + self.active + self.recovery
    }
}

#[derive(Debug, Clone)]
pub struct Ruleset {
    // timing (in ticks; tick_rate itself stays an integer)
    pub tick_rate: i32,
    pub round_ticks: u32,
    pub intermission_ticks: u32,
    pub opening_freeze_ticks: u32,
    pub rounds_to_win: u8,
    pub max_rounds: u8,

    // arena, in Fixed units
    pub arena_width: Fixed,
    pub ceiling: Fixed,
    pub fighter_height: Fixed,
    pub fighter_width: Fixed,
    pub start_separation: Fixed,
    /// Half-width of the range the actual starting separation is drawn
    /// from, uniformly, once per match, from the match seed.
    pub start_separation_jitter: i32,

    /// This ruleset revision's version label (e.g. "2026.2"), bumped with
    /// every deliberate balance/engine change. See tuning/CHANGELOG.md.
    pub version: String,

    pub vitality: i32,
    pub surge_max: i32,
    pub guard_max: i32,

    pub observation_delay: u32,
    pub reflex_interval: u32,

    // movement, converted to per-tick Fixed values at load time
    pub gravity_per_tick: Fixed,
    pub walk_speed_per_tick: Fixed,
    pub dash_speed_per_tick: Fixed,
    pub dash_ticks: u16,
    pub jump_velocity_per_tick: Fixed,

    pub sudden_death_ticks: u32,
    pub sudden_death_vitality: i32,

    pub passivity_warning_ticks: u32,
    pub passivity_penalty_fraction_pct: i32,

    pub guard_crush_ticks: u16,
    pub guard_crush_refill_to: i32,

    pub throw_tech_window_ticks: u32,

    pub light: MoveSpec,
    pub medium: MoveSpec,
    pub heavy: MoveSpec,
    pub throw: MoveSpec,

    pub combo_scaling_pct: Vec<i32>,
    pub combo_floor_pct: i32,
    pub combo_hard_knockdown_hit: u8,

    /// Move names (lowercase: "light"/"medium"/"heavy") exempt from the
    /// non-negative-on-block CI gate. Should stay empty; a whitelisted
    /// entry needs an adjacent TOML comment explaining why.
    pub block_advantage_whitelist: Vec<String>,

    pub light_hitbox: Rect,
    pub medium_hitbox: Rect,
    pub heavy_hitbox: Rect,
    pub throw_hitbox: Rect,
    pub light_hitbox_crouch: Rect,
    pub medium_hitbox_crouch: Rect,
    pub hurt_head: Rect,
    pub hurt_torso: Rect,
    pub hurt_arm_left: Rect,
    pub hurt_arm_right: Rect,
    pub hurt_leg_left: Rect,
    pub hurt_leg_right: Rect,
    pub hurt_head_crouch: Rect,
    pub hurt_torso_crouch: Rect,
    pub hurt_arm_left_crouch: Rect,
    pub hurt_arm_right_crouch: Rect,
    pub hurt_leg_left_crouch: Rect,
    pub hurt_leg_right_crouch: Rect,
}

#[derive(Debug)]
pub enum RulesetError {
    Parse(String),
}

impl fmt::Display for RulesetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RulesetError::Parse(s) => write!(f, "ruleset parse error: {s}"),
        }
    }
}
impl std::error::Error for RulesetError {}

fn rect_from(v: [i32; 4]) -> Rect {
    Rect {
        x: Fixed::from_int(v[0]),
        y: Fixed::from_int(v[1]),
        w: Fixed::from_int(v[2]),
        h: Fixed::from_int(v[3]),
    }
}

fn move_from(m: &RawMove) -> MoveSpec {
    MoveSpec {
        startup: m.startup as u16,
        active: m.active as u16,
        recovery: m.recovery as u16,
        damage: m.damage,
        block_damage: m.block_damage,
        hitstun: m.hitstun as u16,
        blockstun: m.blockstun as u16,
        guard_damage: m.guard_damage,
        hitstop: m.hitstop as u16,
    }
}

impl Ruleset {
    pub fn from_toml_str(s: &str) -> Result<Ruleset, RulesetError> {
        let raw: RawRuleset = toml::from_str(s).map_err(|e| RulesetError::Parse(e.to_string()))?;
        let tick_rate = raw.timing.tick_rate;

        // Convert "units per second" -> "units per tick" in fixed point.
        let per_tick = |units_per_sec: i32| -> Fixed {
            Fixed::from_int(units_per_sec) / Fixed::from_int(tick_rate)
        };

        Ok(Ruleset {
            tick_rate,
            round_ticks: raw.timing.round_ticks as u32,
            intermission_ticks: raw.timing.intermission_ticks as u32,
            opening_freeze_ticks: raw.timing.opening_freeze_ticks as u32,
            rounds_to_win: raw.timing.rounds_to_win as u8,
            max_rounds: raw.timing.max_rounds as u8,

            arena_width: Fixed::from_int(raw.arena.arena_width),
            ceiling: Fixed::from_int(raw.arena.ceiling),
            fighter_height: Fixed::from_int(raw.arena.fighter_height),
            fighter_width: Fixed::from_int(raw.arena.fighter_width),
            start_separation: Fixed::from_int(raw.arena.start_separation),
            start_separation_jitter: raw.arena.start_separation_jitter,
            version: raw.meta.version.clone(),

            vitality: raw.meters.vitality,
            surge_max: raw.meters.surge_max,
            guard_max: raw.meters.guard_max,

            observation_delay: raw.agent.observation_delay as u32,
            reflex_interval: raw.agent.reflex_interval as u32,

            gravity_per_tick: per_tick(raw.movement.gravity),
            walk_speed_per_tick: per_tick(raw.movement.walk_speed),
            dash_speed_per_tick: per_tick(raw.movement.dash_speed),
            dash_ticks: raw.movement.dash_ticks as u16,
            jump_velocity_per_tick: per_tick(raw.movement.jump_velocity),

            sudden_death_ticks: raw.sudden_death.ticks as u32,
            sudden_death_vitality: raw.sudden_death.vitality,

            passivity_warning_ticks: raw.passivity.warning_ticks as u32,
            passivity_penalty_fraction_pct: raw.passivity.penalty_fraction_pct,

            guard_crush_ticks: raw.guard_crush.crush_ticks as u16,
            guard_crush_refill_to: raw.guard_crush.refill_to,

            throw_tech_window_ticks: raw.throw.tech_window_ticks as u32,

            light: move_from(&raw.moves.light),
            medium: move_from(&raw.moves.medium),
            heavy: move_from(&raw.moves.heavy),
            throw: move_from(&raw.moves.throw),

            combo_scaling_pct: raw.combo.scaling_pct.clone(),
            combo_floor_pct: raw.combo.floor_pct,
            combo_hard_knockdown_hit: raw.combo.hard_knockdown_hit as u8,
            block_advantage_whitelist: raw.frame_data.block_advantage_whitelist.clone(),

            light_hitbox: rect_from(raw.hitboxes.light_hitbox),
            medium_hitbox: rect_from(raw.hitboxes.medium_hitbox),
            heavy_hitbox: rect_from(raw.hitboxes.heavy_hitbox),
            throw_hitbox: rect_from(raw.hitboxes.throw_hitbox),
            light_hitbox_crouch: rect_from(raw.hitboxes.light_hitbox_crouch),
            medium_hitbox_crouch: rect_from(raw.hitboxes.medium_hitbox_crouch),
            hurt_head: rect_from(raw.hitboxes.hurt_head),
            hurt_torso: rect_from(raw.hitboxes.hurt_torso),
            hurt_arm_left: rect_from(raw.hitboxes.hurt_arm_left),
            hurt_arm_right: rect_from(raw.hitboxes.hurt_arm_right),
            hurt_leg_left: rect_from(raw.hitboxes.hurt_leg_left),
            hurt_leg_right: rect_from(raw.hitboxes.hurt_leg_right),
            hurt_head_crouch: rect_from(raw.hitboxes.hurt_head_crouch),
            hurt_torso_crouch: rect_from(raw.hitboxes.hurt_torso_crouch),
            hurt_arm_left_crouch: rect_from(raw.hitboxes.hurt_arm_left_crouch),
            hurt_arm_right_crouch: rect_from(raw.hitboxes.hurt_arm_right_crouch),
            hurt_leg_left_crouch: rect_from(raw.hitboxes.hurt_leg_left_crouch),
            hurt_leg_right_crouch: rect_from(raw.hitboxes.hurt_leg_right_crouch),
        })
    }

    pub fn move_spec(&self, kind: crate::types::AttackKind) -> &MoveSpec {
        use crate::types::AttackKind::*;
        match kind {
            Light => &self.light,
            Medium => &self.medium,
            Heavy => &self.heavy,
        }
    }

    /// Deterministic hash of the loaded ruleset, embedded in every replay so
    /// a replay can be tied to the exact ruleset it was recorded against.
    pub fn content_hash(&self) -> u64 {
        let mut h = Fnv1a::new();
        macro_rules! hi {
            ($v:expr) => {
                h.write_i32($v as i32)
            };
        }
        hi!(self.tick_rate);
        hi!(self.round_ticks);
        hi!(self.intermission_ticks);
        hi!(self.opening_freeze_ticks);
        hi!(self.rounds_to_win);
        hi!(self.max_rounds);
        h.write_i32(self.arena_width.raw());
        h.write_i32(self.ceiling.raw());
        h.write_i32(self.fighter_height.raw());
        h.write_i32(self.fighter_width.raw());
        h.write_i32(self.start_separation.raw());
        h.write_i32(self.start_separation_jitter);
        h.write_bytes(self.version.as_bytes());
        hi!(self.vitality);
        hi!(self.surge_max);
        hi!(self.guard_max);
        hi!(self.observation_delay);
        hi!(self.reflex_interval);
        h.write_i32(self.gravity_per_tick.raw());
        h.write_i32(self.walk_speed_per_tick.raw());
        h.write_i32(self.dash_speed_per_tick.raw());
        hi!(self.dash_ticks);
        h.write_i32(self.jump_velocity_per_tick.raw());
        hi!(self.sudden_death_ticks);
        hi!(self.sudden_death_vitality);
        hi!(self.passivity_warning_ticks);
        hi!(self.passivity_penalty_fraction_pct);
        hi!(self.guard_crush_ticks);
        hi!(self.guard_crush_refill_to);
        hi!(self.throw_tech_window_ticks);
        for m in [&self.light, &self.medium, &self.heavy, &self.throw] {
            hi!(m.startup);
            hi!(m.active);
            hi!(m.recovery);
            hi!(m.damage);
            hi!(m.block_damage);
            hi!(m.hitstun);
            hi!(m.blockstun);
            hi!(m.guard_damage);
            hi!(m.hitstop);
        }
        for p in &self.combo_scaling_pct {
            hi!(*p);
        }
        hi!(self.combo_floor_pct);
        hi!(self.combo_hard_knockdown_hit);
        for w in &self.block_advantage_whitelist {
            h.write_bytes(w.as_bytes());
        }
        h.finish()
    }
}

pub const DEFAULT_RULESET_TOML: &str = include_str!("../../../rulesets/2026.1.toml");

pub fn default_ruleset() -> Ruleset {
    Ruleset::from_toml_str(DEFAULT_RULESET_TOML).expect("bundled default ruleset must parse")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_default_ruleset() {
        let rs = default_ruleset();
        assert_eq!(rs.tick_rate, 60);
        assert_eq!(rs.round_ticks, 5400);
        assert_eq!(rs.light.damage, 30);
    }

    #[test]
    fn content_hash_stable() {
        let a = default_ruleset().content_hash();
        let b = default_ruleset().content_hash();
        assert_eq!(a, b);
    }

    #[test]
    fn changing_value_changes_hash() {
        let mut toml_str = DEFAULT_RULESET_TOML.replace("damage = 30", "damage = 31");
        assert_ne!(toml_str, DEFAULT_RULESET_TOML);
        let modified = Ruleset::from_toml_str(&toml_str).unwrap();
        toml_str.clear();
        assert_ne!(modified.content_hash(), default_ruleset().content_hash());
        assert_eq!(modified.light.damage, 31);
    }
}
