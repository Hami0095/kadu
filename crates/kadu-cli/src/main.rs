use std::fmt::Write as _;
use std::fs;
use std::time::Instant;

use kadu_agent::{Agent, Dummy, Rusher};
use kadu_core::ruleset::DEFAULT_RULESET_TOML;
use kadu_core::{default_ruleset, Facing, FighterState, FighterView, Intent, Ruleset, Sim};
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

/// Pulls `expected_aggregate = "0x...."` out of a determinism/expected.toml
/// style file. Deliberately not a full TOML parse: this file's shape is
/// controlled by us and kept trivial on purpose.
fn read_expected_aggregate(path: &str) -> u64 {
    let s = fs::read_to_string(path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"));
    for line in s.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("expected_aggregate") {
            let rest = rest.trim_start().strip_prefix('=').expect("malformed expected_aggregate line").trim();
            let hex = rest.trim_matches('"');
            let hex = hex.strip_prefix("0x").unwrap_or(hex);
            return u64::from_str_radix(hex, 16).expect("expected_aggregate is not valid hex");
        }
    }
    panic!("no expected_aggregate key found in {path}");
}

fn cmd_bench(args: &[String]) {
    let mut matches: u32 = 1000;
    let mut seed: u64 = 1;
    let mut expect_path: Option<String> = None;
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
            "--expect" => {
                expect_path = Some(args[i + 1].clone());
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
    let computed = aggregate.finish();
    println!("matches={matches} elapsed={:.3}s matches/sec={:.1} divergences={divergences}", elapsed.as_secs_f64(), per_sec);
    println!("aggregate_hash={computed:#018x}");

    if divergences > 0 {
        eprintln!("FAIL: {divergences} match(es) did not reproduce their own hash chain");
        std::process::exit(1);
    }

    if let Some(path) = expect_path {
        let expected = read_expected_aggregate(&path);
        if computed != expected {
            eprintln!("FAIL: aggregate mismatch against {path}");
            eprintln!("  expected: {expected:#018x}");
            eprintln!("  computed: {computed:#018x}");
            std::process::exit(1);
        }
        println!("OK: aggregate matches {path}");
    }
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
    let mut path: Option<String> = None;
    let mut replay_to: u32 = 0;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--replay-to" => {
                replay_to = args[i + 1].parse().expect("--replay-to must be an integer tick");
                i += 2;
            }
            other => {
                if path.is_none() {
                    path = Some(other.to_string());
                } else {
                    eprintln!("unknown argument '{other}'");
                }
                i += 1;
            }
        }
    }
    let path = path.expect("usage: kadu watch <replay.json> [--replay-to <tick>]");
    let s = fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"));
    let replay = from_json(&s).unwrap_or_else(|e| panic!("failed to parse replay {path}: {e}"));

    let ruleset = Ruleset::from_toml_str(&replay.ruleset_toml).unwrap_or_else(|_| default_ruleset());
    let width = ruleset.arena_width;
    let mut sim = kadu_core::Sim::new(ruleset, replay.rng_seed);

    const COLS: i64 = 60;

    if replay_to > 0 {
        println!("fast-forwarding silently to tick {replay_to}...");
    }

    for (a, b) in &replay.intent_stream {
        let report = sim.tick([*a, *b]);
        if sim.global_tick() < replay_to {
            continue;
        }
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

/// Re-simulates `replay` from scratch through `intent_stream[0..=index]` and
/// returns (global_tick, round, fighter0, fighter1) at that point. `None`
/// for `index` returns the pre-match initial state (before any tick).
fn state_at(replay: &Replay, index: Option<usize>) -> (u32, u8, FighterView, FighterView) {
    let ruleset = Ruleset::from_toml_str(&replay.ruleset_toml).unwrap_or_else(|_| default_ruleset());
    let mut sim = Sim::new(ruleset, replay.rng_seed);
    if let Some(index) = index {
        for k in 0..=index {
            let (a, b) = replay.intent_stream[k];
            sim.tick([a, b]);
        }
    }
    (sim.global_tick(), sim.round(), sim.fighter_view(0), sim.fighter_view(1))
}

fn fmt_view(v: &FighterView) -> Vec<(&'static str, String)> {
    vec![
        ("position.x".into(), format!("{:?}", v.position.x)),
        ("position.y".into(), format!("{:?}", v.position.y)),
        ("velocity.x".into(), format!("{:?}", v.velocity.x)),
        ("velocity.y".into(), format!("{:?}", v.velocity.y)),
        ("facing".into(), if v.facing == Facing::Right { "Right".to_string() } else { "Left".to_string() }),
        ("vitality".into(), v.vitality.to_string()),
        ("surge".into(), v.surge.to_string()),
        ("guard".into(), v.guard.to_string()),
        ("state".into(), format!("{:?}", v.state)),
        ("state_tick".into(), v.state_tick.to_string()),
        ("combo_count".into(), v.combo_count.to_string()),
        ("airborne".into(), v.airborne.to_string()),
    ]
}

fn print_fighter_diff(label: &str, a: &FighterView, b: &FighterView) {
    println!("  {label}");
    println!("    {:<14} {:<24} {:<24}", "field", "A", "B");
    let fa = fmt_view(a);
    let fb = fmt_view(b);
    for ((name, va), (_, vb)) in fa.iter().zip(fb.iter()) {
        let marker = if va != vb { "  <-- DIFF" } else { "" };
        println!("    {name:<14} {va:<24} {vb:<24}{marker}");
    }
}

fn cmd_diverge(args: &[String]) {
    let mut path_a: Option<String> = None;
    let mut path_b: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--a" => {
                path_a = Some(args[i + 1].clone());
                i += 2;
            }
            "--b" => {
                path_b = Some(args[i + 1].clone());
                i += 2;
            }
            other => {
                eprintln!("unknown argument '{other}'");
                i += 1;
            }
        }
    }
    let path_a = path_a.expect("usage: kadu diverge --a <run_a.json> --b <run_b.json>");
    let path_b = path_b.expect("usage: kadu diverge --a <run_a.json> --b <run_b.json>");

    let replay_a = from_json(&fs::read_to_string(&path_a).unwrap_or_else(|e| panic!("read {path_a}: {e}"))).unwrap_or_else(|e| panic!("parse {path_a}: {e}"));
    let replay_b = from_json(&fs::read_to_string(&path_b).unwrap_or_else(|e| panic!("read {path_b}: {e}"))).unwrap_or_else(|e| panic!("parse {path_b}: {e}"));

    let (Some(ha), Some(hb)) = (&replay_a.tick_hashes, &replay_b.tick_hashes) else {
        eprintln!(
            "error: kadu diverge requires both replays to have been recorded with the `trace-hashes` feature enabled.\n\
             {path_a}: tick_hashes {}\n\
             {path_b}: tick_hashes {}\n\
             Rebuild with `cargo build --release --features trace-hashes -p kadu-cli` and re-record both replays.",
            if replay_a.tick_hashes.is_some() { "present" } else { "MISSING" },
            if replay_b.tick_hashes.is_some() { "present" } else { "MISSING" },
        );
        std::process::exit(1);
    };

    let min_len = ha.len().min(hb.len());
    let mut lo = 0usize;
    let mut hi = min_len;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if ha[mid] == hb[mid] {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }

    if lo == min_len {
        if ha.len() == hb.len() {
            println!("OK: no divergence found. {} ticks compared, all hashes match.", ha.len());
            return;
        }
        println!(
            "no hash mismatch within the first {min_len} ticks, but the replays differ in length: A has {} ticks, B has {} ticks.",
            ha.len(),
            hb.len()
        );
        println!("the shorter replay ended at tick {min_len}; the other continued beyond it.");
        return;
    }

    let diverge_index = lo;
    let before_index = if diverge_index == 0 { None } else { Some(diverge_index - 1) };

    let (tick_before, round_before, a0_before, a1_before) = before_index.map(|idx| state_at(&replay_a, Some(idx))).unwrap_or_else(|| state_at(&replay_a, None));
    let (_, _, b0_before, b1_before) = before_index.map(|idx| state_at(&replay_b, Some(idx))).unwrap_or_else(|| state_at(&replay_b, None));
    let (tick_at, round_at, a0_at, a1_at) = state_at(&replay_a, Some(diverge_index));
    let (_, _, b0_at, b1_at) = state_at(&replay_b, Some(diverge_index));

    println!("first differing tick: hash index {diverge_index} (global tick {tick_at}, round {round_at})");
    println!("A: {path_a}");
    println!("B: {path_b}");
    println!();
    println!("--- tick {tick_before} (round {round_before}, immediately before) ---");
    print_fighter_diff("P1", &a0_before, &b0_before);
    print_fighter_diff("P2", &a1_before, &b1_before);
    println!();
    println!("--- tick {tick_at} (round {round_at}, first differing tick) ---");
    print_fighter_diff("P1", &a0_at, &b0_at);
    print_fighter_diff("P2", &a1_at, &b1_at);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        eprintln!("usage: kadu <run|verify|bench|watch|diverge> [args]");
        std::process::exit(2);
    };
    let rest = &args[1..];
    match cmd.as_str() {
        "run" => cmd_run(rest),
        "verify" => cmd_verify(rest),
        "bench" => cmd_bench(rest),
        "watch" => cmd_watch(rest),
        "diverge" => cmd_diverge(rest),
        other => {
            eprintln!("unknown command '{other}'. usage: kadu <run|verify|bench|watch|diverge> [args]");
            std::process::exit(2);
        }
    }
}
