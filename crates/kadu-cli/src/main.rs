use std::fmt::Write as _;
use std::fs;
use std::time::Instant;

use kadu_agent::{Agent, Dummy, Rusher};
use kadu_core::ruleset::DEFAULT_RULESET_TOML;
use kadu_core::{default_ruleset, FighterState, Intent, Ruleset};
use kadu_replay::{from_json, to_json, verify as verify_replay, Recorder, Replay};

fn make_agent(name: &str) -> Box<dyn Agent> {
    match name {
        "dummy" => Box::new(Dummy),
        "rusher" => Box::new(Rusher::new()),
        other => {
            eprintln!("unknown agent '{other}', falling back to dummy");
            Box::new(Dummy)
        }
    }
}

fn load_ruleset(path: Option<&str>) -> (Ruleset, String) {
    match path {
        Some(p) => {
            let s = fs::read_to_string(p).unwrap_or_else(|e| panic!("failed to read ruleset {p}: {e}"));
            let rs = Ruleset::from_toml_str(&s).unwrap_or_else(|e| panic!("failed to parse ruleset {p}: {e}"));
            (rs, s)
        }
        None => (default_ruleset(), DEFAULT_RULESET_TOML.to_string()),
    }
}

fn run_match(agent_a_name: &str, agent_b_name: &str, seed: u64, ruleset_path: Option<&str>) -> Replay {
    let (ruleset, ruleset_toml) = load_ruleset(ruleset_path);
    let mut agent_a = make_agent(agent_a_name);
    let mut agent_b = make_agent(agent_b_name);
    let reflex_interval = ruleset.reflex_interval;

    let mut rec = Recorder::new(ruleset, ruleset_toml, seed, agent_a_name, agent_b_name);
    let mut last_intents = [Intent::None, Intent::None];

    loop {
        let tick = rec.sim.global_tick();
        if tick % reflex_interval == 0 {
            let obs_a = rec.sim.observation(0);
            let obs_b = rec.sim.observation(1);
            last_intents = [agent_a.decide(&obs_a), agent_b.decide(&obs_b)];
        }
        rec.tick(last_intents);
        if rec.is_over() {
            break;
        }
        if rec.sim.global_tick() > 2_000_000 {
            eprintln!("match exceeded safety tick limit, aborting");
            break;
        }
    }

    rec.into_replay()
}

fn cmd_run(args: &[String]) {
    let mut agent_a = "rusher".to_string();
    let mut agent_b = "dummy".to_string();
    let mut seed: u64 = 42;
    let mut out = "match.json".to_string();
    let mut ruleset_path: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--agent-a" => {
                agent_a = args[i + 1].clone();
                i += 2;
            }
            "--agent-b" => {
                agent_b = args[i + 1].clone();
                i += 2;
            }
            "--seed" => {
                seed = args[i + 1].parse().expect("--seed must be an integer");
                i += 2;
            }
            "--out" => {
                out = args[i + 1].clone();
                i += 2;
            }
            "--ruleset" => {
                ruleset_path = Some(args[i + 1].clone());
                i += 2;
            }
            other => {
                eprintln!("unknown argument '{other}'");
                i += 1;
            }
        }
    }

    let replay = run_match(&agent_a, &agent_b, seed, ruleset_path.as_deref());
    let json = to_json(&replay).expect("serialize replay");
    fs::write(&out, json).unwrap_or_else(|e| panic!("failed to write {out}: {e}"));

    match &replay.result {
        Some(r) => println!(
            "match complete: winner={:?} reason={} chain_hash={:#018x} -> {out}",
            r.winner, r.reason, replay.final_state_hash_chain
        ),
        None => println!("match ended without a recorded result (aborted) -> {out}"),
    }
}

fn cmd_verify(args: &[String]) {
    let path = args.first().expect("usage: kadu verify <replay.json>");
    let s = fs::read_to_string(path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"));
    let replay = from_json(&s).unwrap_or_else(|e| panic!("failed to parse replay {path}: {e}"));
    let result = verify_replay(&replay);
    if result.ok {
        println!("OK: replay verifies. {} ticks, chain_hash={:#018x}", result.ticks_run, result.computed_chain_hash);
    } else {
        println!(
            "FAIL: hash chain mismatch after {} ticks. expected={:#018x} computed={:#018x}",
            result.ticks_run, result.expected_chain_hash, result.computed_chain_hash
        );
        std::process::exit(1);
    }
}

fn cmd_bench(args: &[String]) {
    let mut matches: u32 = 1000;
    let mut seed: u64 = 1;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--matches" => {
                matches = args[i + 1].parse().expect("--matches must be an integer");
                i += 2;
            }
            "--seed" => {
                seed = args[i + 1].parse().expect("--seed must be an integer");
                i += 2;
            }
            other => {
                eprintln!("unknown argument '{other}'");
                i += 1;
            }
        }
    }

    let start = Instant::now();
    let mut divergences = 0u32;
    let mut aggregate = kadu_core::hash::Fnv1a::new();

    for m in 0..matches {
        let s = seed.wrapping_add(m as u64);
        let replay = run_match("rusher", "dummy", s, None);
        let result = verify_replay(&replay);
        if !result.ok {
            divergences += 1;
            eprintln!("hash divergence in match seed={s}");
        }
        aggregate.write_u64(replay.final_state_hash_chain);
    }

    let elapsed = start.elapsed();
    let per_sec = matches as f64 / elapsed.as_secs_f64();
    println!("matches={matches} elapsed={:.3}s matches/sec={:.1} divergences={divergences}", elapsed.as_secs_f64(), per_sec);
    println!("aggregate_hash={:#018x}", aggregate.finish());
}

fn intent_glyph(state: FighterState) -> char {
    match state {
        FighterState::Idle => 'I',
        FighterState::WalkFwd | FighterState::WalkBack => 'W',
        FighterState::Crouch => 'c',
        FighterState::DashFwd | FighterState::DashBack => 'D',
        FighterState::JumpRise | FighterState::JumpFall => 'J',
        FighterState::AttackStartup => 's',
        FighterState::AttackActive => 'A',
        FighterState::AttackRecovery => 'r',
        FighterState::BlockStand | FighterState::BlockCrouch => 'B',
        FighterState::BlockStun => 'b',
        FighterState::HitStun => 'h',
        FighterState::Knockdown => 'K',
        FighterState::WakeUp => 'u',
        FighterState::GuardCrush => 'G',
        FighterState::Dizzy => 'Z',
        FighterState::ThrowAttempt => 'T',
        FighterState::ThrowWhiff => 't',
        FighterState::Thrown => 'X',
        FighterState::RoundFreeze => '.',
        FighterState::RoundOver => 'O',
    }
}

fn cmd_watch(args: &[String]) {
    let path = args.first().expect("usage: kadu watch <replay.json>");
    let s = fs::read_to_string(path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"));
    let replay = from_json(&s).unwrap_or_else(|e| panic!("failed to parse replay {path}: {e}"));

    let ruleset = Ruleset::from_toml_str(&replay.ruleset_toml).unwrap_or_else(|_| default_ruleset());
    let width = ruleset.arena_width;
    let mut sim = kadu_core::Sim::new(ruleset, replay.rng_seed);

    const COLS: i64 = 60;

    for (a, b) in &replay.intent_stream {
        let report = sim.tick([*a, *b]);
        let v0 = sim.fighter_view(0);
        let v1 = sim.fighter_view(1);

        let mut line = vec![b'-'; COLS as usize];
        let to_col = |x: kadu_core::Fixed| -> usize {
            let raw = x.to_int() as i64 * COLS / width.to_int().max(1) as i64;
            raw.clamp(0, COLS - 1) as usize
        };
        line[to_col(v0.position.x)] = b'1';
        line[to_col(v1.position.x)] = b'2';
        let track: String = line.into_iter().map(|c| c as char).collect();

        let mut out = String::new();
        let _ = write!(
            out,
            "t={:>6} r{} [{}] P1 {}{} vit={:>4} grd={:>3} sur={:>3} | P2 {}{} vit={:>4} grd={:>3} sur={:>3}",
            sim.global_tick(),
            sim.round(),
            track,
            intent_glyph(v0.state),
            if v0.combo_count > 0 { format!("x{}", v0.combo_count) } else { String::new() },
            v0.vitality,
            v0.guard,
            v0.surge,
            intent_glyph(v1.state),
            if v1.combo_count > 0 { format!("x{}", v1.combo_count) } else { String::new() },
            v1.vitality,
            v1.guard,
            v1.surge,
        );
        println!("{out}");

        for h in &report.hits {
            println!(
                "    hit: P{} -> P{} dmg={} {}{}",
                h.attacker_idx + 1,
                h.defender_idx + 1,
                h.damage,
                if h.blocked { "(blocked) " } else { "" },
                if h.hard_knockdown { "(hard knockdown)" } else { "" }
            );
        }
        for w in &report.warnings {
            println!("    passivity warning: P{} {}", w.fighter_idx + 1, if w.second_warning { "(2nd, vitality penalty)" } else { "" });
        }
        if let Some(outcome) = report.round_ended {
            println!("    round {} ended: {:?}", sim.round(), outcome);
        }
        if let Some(result) = report.match_ended {
            println!("MATCH OVER: winner={:?} reason={:?}", result.winner, result.reason);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        eprintln!("usage: kadu <run|verify|bench|watch> [args]");
        std::process::exit(2);
    };
    let rest = &args[1..];
    match cmd.as_str() {
        "run" => cmd_run(rest),
        "verify" => cmd_verify(rest),
        "bench" => cmd_bench(rest),
        "watch" => cmd_watch(rest),
        other => {
            eprintln!("unknown command '{other}'. usage: kadu <run|verify|bench|watch> [args]");
            std::process::exit(2);
        }
    }
}
