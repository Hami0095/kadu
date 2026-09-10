//! v0.1 Task 3: each scripted agent falsifies one specific rule from the
//! Codex. These tests are that falsification made executable - if a future
//! balance change breaks one of these, CI says so.
//!
//! Some tests place fighters closer than the default start_separation
//! (600 units) so two deliberately stationary agents (Turtle, Spammer) can
//! actually reach each other; whether they reach each other from the
//! *default* starting distance is exactly the question the v0.1 tournament
//! report (Task 5) answers, not something these per-rule unit tests decide.

use kadu_agent::{Agent, Dummy, Random, Runner, Rusher, Spammer, Turtle};
use kadu_core::ruleset::{Ruleset, DEFAULT_RULESET_TOML};
use kadu_core::{default_ruleset, FighterState, Intent, Sim};

fn ruleset_with(replacements: &[(&str, &str)]) -> Ruleset {
    let mut s = DEFAULT_RULESET_TOML.to_string();
    for (from, to) in replacements {
        assert!(s.contains(from), "pattern {from:?} not found in ruleset TOML");
        s = s.replace(from, to);
    }
    Ruleset::from_toml_str(&s).expect("modified ruleset must still parse")
}

#[allow(dead_code)] // guard_crushes_b isn't read by the current tests but documents the full summary shape
struct MatchSummary {
    winner: Option<u8>,
    guard_crushes_a: u32,
    guard_crushes_b: u32,
    /// (combo hit number after this hit, damage dealt) for every unblocked,
    /// non-throw strike, across both fighters.
    hit_damages: Vec<(u8, i32)>,
    max_combo: u8,
    warnings: u32,
    second_warnings: u32,
}

fn play_match(mut agent_a: Box<dyn Agent>, mut agent_b: Box<dyn Agent>, ruleset: Ruleset, seed: u64) -> MatchSummary {
    let reflex_interval = ruleset.reflex_interval;
    let mut sim = Sim::new(ruleset, seed);
    let mut last_intents = [Intent::None, Intent::None];

    let mut guard_crushes_a = 0u32;
    let mut guard_crushes_b = 0u32;
    let mut hit_damages = Vec::new();
    let mut max_combo = 0u8;
    let mut warnings = 0u32;
    let mut second_warnings = 0u32;
    let mut winner = None;

    for _ in 0..200_000u32 {
        let tick = sim.global_tick();
        if tick % reflex_interval == 0 {
            let obs_a = sim.observation(0);
            let obs_b = sim.observation(1);
            last_intents = [agent_a.decide(&obs_a), agent_b.decide(&obs_b)];
        }
        let report = sim.tick(last_intents);

        for h in &report.hits {
            let defender_view = sim.fighter_view(h.defender_idx);
            if h.blocked && defender_view.state == FighterState::GuardCrush {
                if h.defender_idx == 0 {
                    guard_crushes_a += 1;
                } else {
                    guard_crushes_b += 1;
                }
            }
            if !h.blocked && !h.is_throw {
                max_combo = max_combo.max(defender_view.combo_count);
                hit_damages.push((defender_view.combo_count, h.damage));
            }
        }
        warnings += report.warnings.len() as u32;
        second_warnings += report.warnings.iter().filter(|w| w.second_warning).count() as u32;

        if let Some(m) = report.match_ended {
            winner = m.winner;
            break;
        }
        if sim.global_tick() > 150_000 {
            break;
        }
    }

    MatchSummary { winner, guard_crushes_a, guard_crushes_b, hit_damages, max_combo, warnings, second_warnings }
}

#[test]
fn turtle_gets_guard_crushed() {
    // Both agents are stationary by design, so start them close enough for
    // Spammer's Light to actually reach Turtle without either one moving.
    let rs = ruleset_with(&[("start_separation     = 600", "start_separation     = 120")]);
    let n = 100;
    let mut turtle_wins = 0;
    let mut matches_with_crush = 0;

    for seed in 0..n {
        let summary = play_match(Box::new(Turtle::new()), Box::new(Spammer), rs.clone(), seed);
        if summary.guard_crushes_a > 0 {
            matches_with_crush += 1;
        }
        if summary.winner == Some(0) {
            turtle_wins += 1;
        }
    }

    let crush_rate = matches_with_crush as f64 / n as f64;
    let turtle_win_rate = turtle_wins as f64 / n as f64;
    assert!(crush_rate >= 0.9, "expected Guard Crush in nearly every match, got {crush_rate:.2}");
    assert!(turtle_win_rate < 0.10, "Turtle win rate {turtle_win_rate:.2} >= 10%: blocking is free, anti-turtling is broken");
}

#[test]
fn runner_receives_passivity_penalty() {
    // Small warning window so the retreat-and-block phase reliably crosses
    // it within the match's tick budget.
    let rs = ruleset_with(&[("warning_ticks        = 300", "warning_ticks        = 90")]);
    let n = 100;
    let mut any_warning = 0;
    let mut any_second_warning = 0;

    for seed in 0..n {
        let summary = play_match(Box::new(Runner::new()), Box::new(Dummy), rs.clone(), seed);
        if summary.warnings > 0 {
            any_warning += 1;
        }
        if summary.second_warnings > 0 {
            any_second_warning += 1;
        }
    }

    assert!(any_warning as f64 / n as f64 >= 0.9, "expected passivity warnings against a corner-turtling Runner in nearly every match");
    assert!(any_second_warning as f64 / n as f64 >= 0.5, "expected the vitality penalty (2nd warning) to actually land in a majority of matches");
}

#[test]
fn spammer_combos_are_capped() {
    // Finding, not a test bug: with Light's frame data (startup 4 / active 2
    // / recovery 7, hitstun 12 - unchanged from the original v0 spec) the
    // defender reaches neutral exactly 1 tick before Spammer's next Light
    // lands, so the combo counter resets to 0 before every hit. Light spam
    // against a stationary, non-blocking opponent never naturally chains
    // under the current numbers - that's real output of this milestone,
    // not something to quietly tune away here (out of scope for v0.1).
    // This test therefore asserts what's actually true: the cap is never
    // exceeded, and damage scaling is internally consistent for whatever
    // combo depth does occur, rather than assuming deep combos happen.
    let rs = ruleset_with(&[("start_separation     = 600", "start_separation     = 120")]);
    let n = 100;
    let mut max_combo_seen = 0u8;

    for seed in 0..n {
        let summary = play_match(Box::new(Spammer), Box::new(Dummy), rs.clone(), seed);
        assert!(summary.max_combo <= 15, "combo exceeded the 15-hit hard-knockdown cap: {}", summary.max_combo);
        max_combo_seen = max_combo_seen.max(summary.max_combo);

        let light_damage = default_ruleset().light.damage;
        for (hit_number, damage) in &summary.hit_damages {
            let scale_pct = match hit_number {
                1 => 100,
                2 => 90,
                3 => 80,
                4 => 70,
                5 => 60,
                _ => 50,
            };
            let expected = (light_damage * scale_pct) / 100;
            // A killing blow is clamped to whatever vitality remains, so it
            // can legitimately deal less than the formula's raw output -
            // never more.
            assert!(
                *damage > 0 && *damage <= expected,
                "hit #{hit_number} dealt {damage}, expected <= {expected} ({scale_pct}% of {light_damage})"
            );
        }
    }
    eprintln!("spammer_combos_are_capped: max combo length observed across {n} matches = {max_combo_seen}");
}

#[test]
fn rusher_beats_random() {
    let n = 100;
    let mut rusher_wins = 0;
    for seed in 0..n {
        let summary = play_match(Box::new(Rusher::new()), Box::new(Random::new(seed, 1)), default_ruleset(), seed);
        if summary.winner == Some(0) {
            rusher_wins += 1;
        }
    }
    let rate = rusher_wins as f64 / n as f64;
    assert!(rate > 0.70, "Rusher win rate against Random was {rate:.2}, expected > 70% (Rusher would be broken)");
}

#[test]
fn random_beats_dummy() {
    let n = 100;
    let mut random_wins = 0;
    for seed in 0..n {
        let summary = play_match(Box::new(Random::new(seed, 0)), Box::new(Dummy), default_ruleset(), seed);
        if summary.winner == Some(0) {
            random_wins += 1;
        }
    }
    let rate = random_wins as f64 / n as f64;
    assert!(rate > 0.90, "Random win rate against Dummy was {rate:.2}, expected > 90% (any agent should crush a no-op opponent)");
}
