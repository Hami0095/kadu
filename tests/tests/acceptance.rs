//! v0 acceptance-criteria tests: one dedicated test per rule that must
//! break the build if the rule is removed, plus the round/bout ending
//! variants (KO, timeout, double KO, sudden death) and ruleset tunability.

use kadu_core::combat;
use kadu_core::fighter::Fighter;
use kadu_core::fixed::Vec2;
use kadu_core::ruleset::{Ruleset, DEFAULT_RULESET_TOML};
use kadu_core::{default_ruleset, AttackKind, Direction, Facing, FighterState, Intent, MatchEndReason, RoundOutcome, Sim};

fn ruleset_with(replacements: &[(&str, &str)]) -> Ruleset {
    let mut s = DEFAULT_RULESET_TOML.to_string();
    for (from, to) in replacements {
        assert!(s.contains(from), "pattern {from:?} not found in ruleset TOML");
        s = s.replace(from, to);
    }
    Ruleset::from_toml_str(&s).expect("modified ruleset must still parse")
}

fn make_pair(rs: &Ruleset, dx: i32) -> [Fighter; 2] {
    use kadu_core::fixed::Fixed;
    let a = Fighter::new(Vec2::new(Fixed::from_int(200), Fixed::ZERO), Facing::Right, rs.vitality, rs.guard_max);
    let mut b = Fighter::new(Vec2::new(Fixed::from_int(200 + dx), Fixed::ZERO), Facing::Left, rs.vitality, rs.guard_max);
    b.state = FighterState::Idle;
    [a, b]
}

fn set_attacking(f: &mut Fighter, kind: AttackKind, crouching: bool, jumping: bool) {
    f.state = FighterState::AttackActive;
    f.state_tick = 1;
    f.attack_kind = Some(kind);
    f.attack_crouching = crouching;
    f.attack_jumping = jumping;
    f.attack_hit_registered = false;
}

/// Drives a full match between two scripted-intent closures until it ends,
/// returning the final report and the sim.
fn run_scripted(rs: Ruleset, seed: u64, mut policy: impl FnMut(&Sim) -> [Intent; 2], max_ticks: u32) -> (Sim, Option<kadu_core::TickReport>) {
    let mut sim = Sim::new(rs, seed);
    let mut last_report = None;
    for _ in 0..max_ticks {
        let intents = policy(&sim);
        let report = sim.tick(intents);
        let over = sim.is_over();
        last_report = Some(report);
        if over {
            break;
        }
    }
    (sim, last_report)
}

/// Walks fighter 0 forward until close, then spams the given attack; fighter
/// 1 never acts. Used to reach a KO deterministically.
fn attacker_vs_dummy_policy(kind: AttackKind) -> impl FnMut(&Sim) -> [Intent; 2] {
    move |sim: &Sim| {
        let obs = sim.observation(0);
        let mine = obs.me.state;
        let intent0 = if mine.accepts_new_intent() {
            if obs.distance.to_int() > 120 {
                Intent::Move(Direction::Forward)
            } else {
                Intent::Attack(kind)
            }
        } else {
            Intent::None
        };
        [intent0, Intent::None]
    }
}

// ---------------------------------------------------------------------
// Round/bout endings
// ---------------------------------------------------------------------

#[test]
fn round_ends_in_ko() {
    let rs = default_ruleset();
    let (_, report) = run_scripted(rs, 1, attacker_vs_dummy_policy(AttackKind::Heavy), 20_000);
    let report = report.expect("at least one tick ran");
    let m = report.match_ended.expect("match should have ended");
    assert_eq!(m.winner, Some(0));
    assert_eq!(m.reason, MatchEndReason::Ko);
}

#[test]
fn round_times_out_with_percentage_winner() {
    // Small round clock and a single decisive round; fighter 0 lands exactly
    // one light hit then stalls, so the round (and match) expires with
    // fighter 0 ahead on vitality percentage.
    let rs = ruleset_with(&[
        ("round_ticks          = 3600    # 60 seconds (v0.2 Change 4: 5400 -> 3600; prefer shortening the round over inflating damage - timeouts are a spectator problem before a balance problem)", "round_ticks          = 260"),
        ("rounds_to_win        = 2", "rounds_to_win        = 1"),
    ]);
    let mut hit_landed = false;
    let (_, report) = run_scripted(
        rs,
        2,
        move |sim: &Sim| {
            let obs = sim.observation(0);
            if hit_landed {
                return [Intent::None, Intent::None];
            }
            let intent0 = if obs.me.state.accepts_new_intent() {
                if obs.distance.to_int() > 120 {
                    Intent::Move(Direction::Forward)
                } else {
                    hit_landed = true;
                    Intent::Attack(AttackKind::Light)
                }
            } else {
                Intent::None
            };
            [intent0, Intent::None]
        },
        5000,
    );
    let report = report.expect("ticks ran");
    let m = report.match_ended.expect("match should have ended by timeout");
    assert_eq!(m.reason, MatchEndReason::Timeout);
    assert_eq!(m.winner, Some(0));
}

#[test]
fn simultaneous_lethal_trade_is_a_double_ko() {
    // Fighters start in range with low vitality; both throw a Heavy on the
    // same tick. Collision resolution uses a pre-damage snapshot, so this is
    // a genuine trade: both connect and both are KO'd on the same tick.
    let rs = ruleset_with(&[
        ("vitality             = 1000", "vitality             = 50"),
        ("start_separation     = 600", "start_separation     = 100"),
        ("start_separation_jitter = 80", "start_separation_jitter = 0"),
    ]);
    let mut sim = Sim::new(rs, 3);
    let mut round_ended = None;
    for _ in 0..2000 {
        let report = sim.tick([Intent::Attack(AttackKind::Heavy), Intent::Attack(AttackKind::Heavy)]);
        if report.round_ended.is_some() {
            round_ended = report.round_ended;
            break;
        }
    }
    assert_eq!(round_ended, Some(RoundOutcome::DoubleKo));
}

#[test]
fn sudden_death_triggers_when_bout_reaches_max_rounds_undecided() {
    // Two dummies never fight: every round is an exact draw, so the bout
    // must reach max_rounds and fall into sudden death.
    let rs = ruleset_with(&[
        ("round_ticks          = 3600    # 60 seconds (v0.2 Change 4: 5400 -> 3600; prefer shortening the round over inflating damage - timeouts are a spectator problem before a balance problem)", "round_ticks          = 120"),
        ("intermission_ticks   = 480", "intermission_ticks   = 10"),
        ("opening_freeze_ticks = 90", "opening_freeze_ticks = 5"),
        ("max_rounds           = 5", "max_rounds           = 2"),
    ]);
    let (_, report) = run_scripted(rs, 4, |_sim: &Sim| [Intent::None, Intent::None], 5000);
    let report = report.expect("ticks ran");
    let m = report.match_ended.expect("match should have ended");
    assert_eq!(m.reason, MatchEndReason::SuddenDeath);
}

// ---------------------------------------------------------------------
// Individual rules, exercised directly through combat::resolve
// ---------------------------------------------------------------------

#[test]
fn damage_scaling_follows_the_combo_table() {
    let rs = default_ruleset();
    let expected_pct = [100, 90, 80, 70, 60, 50, 50, 50];
    for (hit_number, pct) in expected_pct.iter().enumerate() {
        let mut fighters = make_pair(&rs, 60);
        set_attacking(&mut fighters[0], AttackKind::Light, false, false);
        fighters[1].vitality = 1000;
        fighters[1].combo_count = hit_number as u8;
        let logs = combat::resolve(&mut fighters, &rs, 0);
        let hit = logs.iter().find(|h| h.attacker_idx == 0).expect("hit should land");
        let expected = (rs.light.damage * pct) / 100;
        assert_eq!(hit.damage, expected, "hit #{hit_number}");
    }
}

#[test]
fn fifteenth_hit_forces_a_hard_knockdown() {
    let rs = default_ruleset();
    let mut fighters = make_pair(&rs, 60);
    set_attacking(&mut fighters[0], AttackKind::Light, false, false);
    fighters[1].vitality = 100_000; // ensure it survives to register the knockdown, not a KO
    fighters[1].combo_count = (rs.combo_hard_knockdown_hit - 1) as u8;
    let logs = combat::resolve(&mut fighters, &rs, 0);
    let hit = logs.iter().find(|h| h.attacker_idx == 0).unwrap();
    assert!(hit.hard_knockdown);
    assert_eq!(fighters[1].state, FighterState::Knockdown);
}

#[test]
fn chip_damage_never_drops_vitality_below_one() {
    let rs = default_ruleset();
    let mut fighters = make_pair(&rs, 60);
    set_attacking(&mut fighters[0], AttackKind::Medium, false, false);
    fighters[1].state = FighterState::BlockStand;
    fighters[1].vitality = 3;
    combat::resolve(&mut fighters, &rs, 0);
    assert_eq!(fighters[1].vitality, 1);
}

#[test]
fn guard_reaching_zero_enters_guard_crush() {
    let rs = default_ruleset();
    let mut fighters = make_pair(&rs, 60);
    set_attacking(&mut fighters[0], AttackKind::Medium, false, false);
    fighters[1].state = FighterState::BlockStand;
    fighters[1].guard = 5; // less than medium's guard_damage of 12
    combat::resolve(&mut fighters, &rs, 0);
    assert_eq!(fighters[1].state, FighterState::GuardCrush);
    assert_eq!(fighters[1].guard, 0);
}

#[test]
fn simultaneous_throws_within_tech_window_deal_no_damage() {
    let rs = default_ruleset();
    let mut fighters = make_pair(&rs, 60);
    for f in fighters.iter_mut() {
        f.state = FighterState::ThrowAttempt;
        f.state_tick = rs.throw.startup + 1;
        f.last_throw_attempt_tick = Some(0);
        f.attack_hit_registered = false;
    }
    let v0_before = fighters[0].vitality;
    let v1_before = fighters[1].vitality;
    let x0_before = fighters[0].position.x;
    let x1_before = fighters[1].position.x;
    let logs = combat::resolve(&mut fighters, &rs, 0);
    // A tech logs a zero-damage HitLog per fighter (for stats: "throws
    // teched"), but deals no damage and applies no hitstun/knockdown.
    assert!(logs.iter().all(|h| h.is_tech && h.damage == 0), "a teched throw should log only zero-damage tech events, got {:?}", logs.iter().map(|h| (h.attacker_idx, h.is_tech, h.damage)).collect::<Vec<_>>());
    assert_eq!(logs.len(), 2, "expected one tech entry per fighter");
    assert_eq!(fighters[0].vitality, v0_before);
    assert_eq!(fighters[1].vitality, v1_before);
    assert!(fighters[0].position.x < x0_before);
    assert!(fighters[1].position.x > x1_before);
}

#[test]
fn passivity_warning_is_logged_within_the_window() {
    let rs = ruleset_with(&[("warning_ticks        = 480   # v0.2 Change 3: 300 -> 480 (8s); a penalty everyone pays is a tax, not a penalty", "warning_ticks        = 60")]);
    let mut sim = Sim::new(rs, 6);
    let mut hit_landed = false;
    let mut warned = false;
    for _ in 0..2000 {
        let obs = sim.observation(0);
        let intents = if !hit_landed {
            if obs.me.state.accepts_new_intent() {
                if obs.distance.to_int() > 120 {
                    [Intent::Move(Direction::Forward), Intent::None]
                } else {
                    hit_landed = true;
                    [Intent::Attack(AttackKind::Light), Intent::None]
                }
            } else {
                [Intent::None, Intent::None]
            }
        } else {
            [Intent::None, Intent::None]
        };
        let report = sim.tick(intents);
        if report.warnings.iter().any(|w| w.fighter_idx == 0) {
            warned = true;
            break;
        }
        if sim.is_over() {
            break;
        }
    }
    assert!(warned, "expected a passivity warning against the leading, stalling fighter");
}

// ---------------------------------------------------------------------
// Ruleset tunability
// ---------------------------------------------------------------------

#[test]
fn changing_a_ruleset_value_changes_behaviour_without_recompiling() {
    let default_rs = default_ruleset();
    let doubled_rs = ruleset_with(&[("damage = 30", "damage = 60")]);
    assert_eq!(doubled_rs.light.damage, 60);

    let mut fighters_a = make_pair(&default_rs, 60);
    set_attacking(&mut fighters_a[0], AttackKind::Light, false, false);
    let logs_a = combat::resolve(&mut fighters_a, &default_rs, 0);

    let mut fighters_b = make_pair(&doubled_rs, 60);
    set_attacking(&mut fighters_b[0], AttackKind::Light, false, false);
    let logs_b = combat::resolve(&mut fighters_b, &doubled_rs, 0);

    assert_eq!(logs_a[0].damage * 2, logs_b[0].damage);
}
