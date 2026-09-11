//! Regenerates tests/corpus/*.json: one replay per round-ending type, plus
//! a long-combo and a guard-crush scenario. Run with:
//!   cargo run -p kadu-cli --example gen_corpus
//! These are hand-scripted intent sequences against raw kadu-core::Sim, not
//! the named agents in kadu-agent, so the corpus doesn't depend on agent
//! behaviour and stays a stable regression net even if agents change.

use std::fs;
use std::path::Path;

use kadu_core::ruleset::{Ruleset, DEFAULT_RULESET_TOML};
use kadu_core::{default_ruleset, AttackKind, Direction, Intent};
use kadu_replay::{to_json, Recorder};

fn ruleset_with(replacements: &[(&str, &str)]) -> (Ruleset, String) {
    let mut s = DEFAULT_RULESET_TOML.to_string();
    for (from, to) in replacements {
        assert!(s.contains(from), "pattern {from:?} not in ruleset TOML");
        s = s.replace(from, to);
    }
    let rs = Ruleset::from_toml_str(&s).expect("modified ruleset must parse");
    (rs, s)
}

fn write_replay(name: &str, rec: Recorder) {
    let replay = rec.into_replay();
    let json = to_json(&replay).expect("serialize");
    let path = Path::new("tests/corpus").join(name);
    fs::write(&path, json).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    println!(
        "{name}: result={:?} chain_hash={:#018x}",
        replay.result.as_ref().map(|r| (r.winner, r.reason.clone())),
        replay.final_state_hash_chain
    );
}

/// Walks fighter 0 forward until in range, then attacks; runs until `done`
/// says stop or the match ends.
fn run_until<F: FnMut(&kadu_core::Sim) -> bool>(rec: &mut Recorder, mut intents_for: impl FnMut(&kadu_core::Sim) -> [Intent; 2], mut done: F, max_ticks: u32) {
    for _ in 0..max_ticks {
        let intents = intents_for(&rec.sim);
        rec.tick(intents);
        if rec.is_over() || done(&rec.sim) {
            break;
        }
    }
}

fn approach_then(kind: AttackKind, threshold: i32) -> impl FnMut(&kadu_core::Sim) -> [Intent; 2] {
    move |sim: &kadu_core::Sim| {
        let obs = sim.observation(0);
        let intent0 = if obs.me.state.accepts_new_intent() {
            if obs.distance.to_int() > threshold {
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

fn main() {
    fs::create_dir_all("tests/corpus").expect("create tests/corpus");

    // 1. KO
    {
        let rs = default_ruleset();
        let mut rec = Recorder::new(rs, DEFAULT_RULESET_TOML.to_string(), 1, "rusher-script", "dummy");
        run_until(&mut rec, approach_then(AttackKind::Heavy, 120), |_| false, 20_000);
        write_replay("ko.json", rec);
    }

    // 2. Timeout (fighter 0 lands one light hit, then stalls; small clock,
    //    single decisive round so the match ends on the timeout).
    {
        let (rs, toml) = ruleset_with(&[
            ("round_ticks          = 3600    # 60 seconds (v0.2 Change 4: 5400 -> 3600; prefer shortening the round over inflating damage - timeouts are a spectator problem before a balance problem)", "round_ticks          = 260"),
            ("rounds_to_win        = 2", "rounds_to_win        = 1"),
        ]);
        let mut rec = Recorder::new(rs, toml, 2, "one-hit-then-stall", "dummy");
        let mut hit_landed = false;
        run_until(
            &mut rec,
            move |sim: &kadu_core::Sim| {
                if hit_landed {
                    return [Intent::None, Intent::None];
                }
                let obs = sim.observation(0);
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
            |_| false,
            5000,
        );
        write_replay("timeout.json", rec);
    }

    // 3. Double KO: low vitality, close start, both spam Heavy. max_rounds=1
    //    so a double-KO round (which awards no one a win) falls straight
    //    into sudden death, and the trade repeats there to end the match.
    {
        let (rs, toml) = ruleset_with(&[
            ("vitality             = 1000", "vitality             = 50"),
            ("start_separation     = 600", "start_separation     = 100"),
            ("start_separation_jitter = 80", "start_separation_jitter = 0"),
            ("max_rounds           = 5", "max_rounds           = 1"),
        ]);
        let mut rec = Recorder::new(rs, toml, 3, "trader", "trader");
        run_until(&mut rec, |_sim| [Intent::Attack(AttackKind::Heavy), Intent::Attack(AttackKind::Heavy)], |_| false, 4000);
        write_replay("double_ko.json", rec);
    }

    // 4. Sudden death: two dummies, tiny clock, low max_rounds, no one ever
    //    acts, so every round is an exact draw until sudden death triggers.
    {
        let (rs, toml) = ruleset_with(&[
            ("round_ticks          = 3600    # 60 seconds (v0.2 Change 4: 5400 -> 3600; prefer shortening the round over inflating damage - timeouts are a spectator problem before a balance problem)", "round_ticks          = 120"),
            ("intermission_ticks   = 480", "intermission_ticks   = 10"),
            ("opening_freeze_ticks = 90", "opening_freeze_ticks = 5"),
            ("max_rounds           = 5", "max_rounds           = 2"),
        ]);
        let mut rec = Recorder::new(rs, toml, 4, "dummy", "dummy");
        run_until(&mut rec, |_sim| [Intent::None, Intent::None], |_| false, 5000);
        write_replay("sudden_death.json", rec);
    }

    // 5. Long combo: spam Light against a Dummy that never blocks or moves,
    //    driving the combo counter up to the hard-knockdown cap.
    {
        let rs = default_ruleset();
        let mut rec = Recorder::new(rs, DEFAULT_RULESET_TOML.to_string(), 5, "spammer-script", "dummy");
        run_until(&mut rec, approach_then(AttackKind::Light, 120), |_| false, 20_000);
        write_replay("long_combo.json", rec);
    }

    // 6. Guard crush: attacker spams Heavy, defender blocks from the start.
    {
        let rs = default_ruleset();
        let mut rec = Recorder::new(rs, DEFAULT_RULESET_TOML.to_string(), 6, "spammer-script", "turtle-script");
        let mut policy = approach_then(AttackKind::Heavy, 160);
        run_until(
            &mut rec,
            move |sim: &kadu_core::Sim| {
                let [intent0, _] = policy(sim);
                [intent0, Intent::Block]
            },
            |_| false,
            20_000,
        );
        write_replay("guard_crush.json", rec);
    }
}
