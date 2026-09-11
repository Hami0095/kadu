//! `kadu frames`: prints frame data computed from the ruleset (not from
//! hand arithmetic), for every move, plus a non-negative-on-block gate that
//! CI runs so a move like v0's original Light (0 on block - a true
//! infinite block string) can never reach main silently again.

use kadu_core::ruleset::{MoveSpec, Ruleset};

pub struct FrameRow {
    pub name: &'static str,
    pub startup: u16,
    pub active: u16,
    pub recovery: u16,
    pub total: u16,
    pub damage: i32,
    pub block_damage: i32,
    pub guard_damage: i32,
    pub hitstun: u16,
    pub blockstun: u16,
    pub hitstop: u16,
    /// None for moves with no block interaction (the throw).
    pub advantage_on_block: Option<i32>,
    pub advantage_on_hit: i32,
    pub reactable: bool,
    pub whitelisted: bool,
}

/// The frame-advantage formula, spelled out once so `kadu frames` and any
/// test computing it agree by construction:
///
///   hit_frame           = startup + 1
///   attacker_actionable = startup + active + recovery
///   defender_actionable = hit_frame + blockstun (or + hitstun on hit)
///   advantage            = defender_actionable - attacker_actionable
///
/// Hitstop freezes both fighters equally and cancels out of the
/// subtraction, so it deliberately does not appear here.
fn advantage(spec: &MoveSpec, defender_stun: u16) -> i32 {
    let hit_frame = spec.startup as i32 + 1;
    let attacker_actionable = spec.startup as i32 + spec.active as i32 + spec.recovery as i32;
    let defender_actionable = hit_frame + defender_stun as i32;
    defender_actionable - attacker_actionable
}

fn row(name: &'static str, spec: &MoveSpec, has_block: bool, worst_case_latency: u32, whitelist: &[String]) -> FrameRow {
    let advantage_on_block = if has_block { Some(advantage(spec, spec.blockstun)) } else { None };
    let advantage_on_hit = advantage(spec, spec.hitstun);
    let reactable = spec.startup as u32 >= worst_case_latency;
    let whitelisted = whitelist.iter().any(|w| w.eq_ignore_ascii_case(name));
    FrameRow {
        name,
        startup: spec.startup,
        active: spec.active,
        recovery: spec.recovery,
        total: spec.total_frames(),
        damage: spec.damage,
        block_damage: spec.block_damage,
        guard_damage: spec.guard_damage,
        hitstun: spec.hitstun,
        blockstun: spec.blockstun,
        hitstop: spec.hitstop,
        advantage_on_block,
        advantage_on_hit,
        reactable,
        whitelisted,
    }
}

pub fn compute(rs: &Ruleset) -> Vec<FrameRow> {
    let worst_case_latency = rs.observation_delay + rs.reflex_interval;
    vec![
        row("light", &rs.light, true, worst_case_latency, &rs.block_advantage_whitelist),
        row("medium", &rs.medium, true, worst_case_latency, &rs.block_advantage_whitelist),
        row("heavy", &rs.heavy, true, worst_case_latency, &rs.block_advantage_whitelist),
        row("throw", &rs.throw, false, worst_case_latency, &rs.block_advantage_whitelist),
    ]
}

pub fn print_table(rows: &[FrameRow]) {
    println!(
        "{:<8} {:>7} {:>6} {:>8} {:>6}  {:>6} {:>5} {:>5}  {:>7} {:>9} {:>7}  {:>9} {:>7}  {:>9}",
        "move", "startup", "active", "recovery", "total", "damage", "chip", "guard", "hitstun", "blockstun", "hitstop", "adv/block", "adv/hit", "reactable"
    );
    for r in rows {
        let adv_block = match r.advantage_on_block {
            Some(a) => format!("{a:+}"),
            None => "n/a".to_string(),
        };
        println!(
            "{:<8} {:>7} {:>6} {:>8} {:>6}  {:>6} {:>5} {:>5}  {:>7} {:>9} {:>7}  {:>9} {:>+7}  {:>9}",
            r.name, r.startup, r.active, r.recovery, r.total, r.damage, r.block_damage, r.guard_damage, r.hitstun, r.blockstun, r.hitstop, adv_block, r.advantage_on_hit, r.reactable
        );
    }
}

/// Returns the names of moves that violate the non-negative-on-block gate
/// (advantage_on_block >= 0) and are not whitelisted. Empty means the gate
/// passes.
pub fn check_violations(rows: &[FrameRow]) -> Vec<String> {
    rows.iter()
        .filter_map(|r| match r.advantage_on_block {
            Some(a) if a >= 0 && !r.whitelisted => Some(format!("{} is {a:+} on block (must be negative, or whitelisted with a reason)", r.name)),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kadu_core::default_ruleset;
    use kadu_core::ruleset::{Ruleset, DEFAULT_RULESET_TOML};

    /// The build prompt's own known values for v0's *original* Light frame
    /// data (startup 4/active 2/recovery 7): "Light 0, Medium -4, Heavy -9
    /// on block." Verified against a reconstructed original-Light ruleset
    /// rather than the shipped default, since v0.2 Change 2 deliberately
    /// moves Light's recovery (see below) - if this test ever fails, the
    /// formula is wrong, not the table.
    #[test]
    fn reproduces_known_block_advantage_values_for_original_light() {
        let s = DEFAULT_RULESET_TOML.replace(
            "recovery = 10   # v0.2 Change 2: 7 -> 10, makes Light -3 on block instead of the +0 infinite block string it was",
            "recovery = 7",
        );
        assert_ne!(s, DEFAULT_RULESET_TOML, "expected to find and replace Light's recovery line");
        let rs = Ruleset::from_toml_str(&s).unwrap();
        let rows = compute(&rs);
        let by_name = |name: &str| rows.iter().find(|r| r.name == name).unwrap();
        assert_eq!(by_name("light").advantage_on_block, Some(0));
        assert_eq!(by_name("medium").advantage_on_block, Some(-4));
        assert_eq!(by_name("heavy").advantage_on_block, Some(-9));
    }

    /// v0.2 Change 2: Light's recovery moved from 7 to 10 specifically to
    /// fix the +0-on-block infinite block string kadu frames --check first
    /// caught. The shipped ruleset must never regress to non-negative.
    #[test]
    fn light_is_no_longer_a_violation_after_change_2() {
        let rs = default_ruleset();
        let rows = compute(&rs);
        assert_eq!(check_violations(&rows), Vec::<String>::new());
    }
}
