//! Round-robin tournament runner. Compresses "is this a good fight?" from a
//! vague feeling into one command that runs in seconds.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;

use serde::Serialize;

use crate::stats::{run_match_with_stats, DISTANCE_BUCKETS};
use crate::{load_ruleset, make_agent, AGENT_NAMES};

#[derive(Default, Clone, Serialize)]
struct PairCell {
    wins_a: u32,
    wins_b: u32,
    draws: u32,
}

#[derive(Default)]
struct Aggregate {
    bouts: u32,
    rounds: u32,
    ko: u32,
    timeout: u32,
    double_ko: u32,
    sudden_death: u32,
    round_duration_ticks_sum: u64,
    damage_per_bout_sum: u64,
    guard_crushes: u32,
    trades: u32,
    passivity_penalties: u32,
    distance_ticks: [u64; DISTANCE_BUCKETS],
    total_ticks: u64,
}

#[derive(Serialize)]
struct TourneyOutput {
    agents: Vec<String>,
    repeats: u32,
    seed: u64,
    matrix: BTreeMap<String, BTreeMap<String, PairCellOut>>,
    win_rates: Vec<(String, f64)>,
    ending_pct: BTreeMap<String, f64>,
    mean_round_duration_ticks: f64,
    mean_round_duration_seconds: f64,
    mean_damage_per_bout: f64,
    guard_crushes: u32,
    guard_crushes_per_100_rounds: f64,
    trades: u32,
    trades_per_100_rounds: f64,
    passivity_penalties: u32,
    distance_histogram_pct: [f64; DISTANCE_BUCKETS],
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct PairCellOut {
    win_pct_row: f64,
    win_pct_col: f64,
    draw_pct: f64,
}

/// The Codex mirror rule: cell (i, j) and its mirror (j, i) must be the
/// same underlying match, replayed with sides swapped - same starting
/// separation draw, same everything except which agent occupies which
/// slot - not just two independently-seeded samples that happen to
/// average out. Symmetric in (i, j) (sorted before hashing) so both
/// directions of a pairing share one seed at each repeat index.
fn pairing_seed(base_seed: u64, i: usize, j: usize, k: u32) -> u64 {
    let (lo, hi) = if i <= j { (i, j) } else { (j, i) };
    base_seed ^ ((lo as u64) << 40) ^ ((hi as u64) << 20) ^ (k as u64)
}

pub fn cmd_tourney(args: &[String]) {
    let mut agents_arg = "all".to_string();
    let mut repeats: u32 = 100;
    let mut seed: u64 = 1;
    let mut out: Option<String> = None;
    let mut ruleset_path: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--agents" => {
                agents_arg = args[i + 1].clone();
                i += 2;
            }
            "--repeats" => {
                repeats = args[i + 1].parse().expect("--repeats must be an integer");
                i += 2;
            }
            "--seed" => {
                seed = args[i + 1].parse().expect("--seed must be an integer");
                i += 2;
            }
            "--out" => {
                out = Some(args[i + 1].clone());
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

    let agents: Vec<String> = if agents_arg == "all" { AGENT_NAMES.iter().map(|s| s.to_string()).collect() } else { agents_arg.split(',').map(|s| s.trim().to_string()).collect() };
    let n = agents.len();

    let mut matrix = vec![vec![PairCell::default(); n]; n];
    let mut wins = vec![0u32; n];
    let mut games = vec![0u32; n];
    let mut agg = Aggregate::default();

    let start = std::time::Instant::now();
    let mut bouts_done = 0u32;
    let total_bouts = (n * n) as u32 * repeats;

    for i in 0..n {
        for j in 0..n {
            for k in 0..repeats {
                let s = pairing_seed(seed, i, j, k);
                let (ruleset, ruleset_toml) = load_ruleset(ruleset_path.as_deref());
                let agent_a = make_agent(&agents[i], s, 0);
                let agent_b = make_agent(&agents[j], s, 1);
                let (replay, stats) = run_match_with_stats(agent_a, agent_b, ruleset, s, ruleset_toml, &agents[i], &agents[j]);

                agg.bouts += 1;
                match replay.result.as_ref().and_then(|r| r.winner) {
                    Some(0) => {
                        matrix[i][j].wins_a += 1;
                        wins[i] += 1;
                    }
                    Some(1) => {
                        matrix[i][j].wins_b += 1;
                        wins[j] += 1;
                    }
                    _ => matrix[i][j].draws += 1,
                }
                games[i] += 1;
                if i != j {
                    games[j] += 1;
                }

                let bout_damage = stats.totals[0].damage.dealt + stats.totals[1].damage.dealt;
                agg.damage_per_bout_sum += bout_damage as u64;
                agg.guard_crushes += stats.totals[0].guard.crushes + stats.totals[1].guard.crushes;
                agg.trades += stats.total_trades;
                agg.passivity_penalties += stats.totals[0].passivity.penalties + stats.totals[1].passivity.penalties;

                for r in &stats.rounds {
                    agg.rounds += 1;
                    agg.round_duration_ticks_sum += r.duration_ticks as u64;
                    match r.ending.as_str() {
                        "ko" => agg.ko += 1,
                        "timeout" => agg.timeout += 1,
                        "double_ko" => agg.double_ko += 1,
                        "sudden_death" => agg.sudden_death += 1,
                        _ => {}
                    }
                    for b in 0..DISTANCE_BUCKETS {
                        agg.distance_ticks[b] += r.distance_histogram[b] as u64;
                        agg.total_ticks += r.distance_histogram[b] as u64;
                    }
                }

                bouts_done += 1;
            }
        }
    }
    let elapsed = start.elapsed();
    eprintln!("tourney: {bouts_done}/{total_bouts} bouts in {:.2}s", elapsed.as_secs_f64());

    // --- win matrix ---
    let name_w = agents.iter().map(|a| a.len()).max().unwrap_or(4).max(6);
    let mut out_text = String::new();
    let _ = write!(out_text, "{:width$}", "", width = name_w + 2);
    for a in &agents {
        let _ = write!(out_text, "{:>10}", a);
    }
    out_text.push('\n');
    for i in 0..n {
        let _ = write!(out_text, "{:width$}", agents[i], width = name_w + 2);
        for j in 0..n {
            let cell = &matrix[i][j];
            let pct = if repeats > 0 { cell.wins_a as f64 * 100.0 / repeats as f64 } else { 0.0 };
            let _ = write!(out_text, "{:>9.1}%", pct);
        }
        out_text.push('\n');
    }
    println!("Win matrix (row's win % vs column, {repeats} bouts per pairing):");
    println!("{out_text}");

    let mut ranked: Vec<(String, f64)> = (0..n)
        .map(|i| {
            let rate = if games[i] > 0 { wins[i] as f64 * 100.0 / games[i] as f64 } else { 0.0 };
            (agents[i].clone(), rate)
        })
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    println!("Overall win rate (ranked):");
    for (name, rate) in &ranked {
        println!("  {name:<10} {rate:>6.1}%");
    }
    println!();

    let ending_total = (agg.ko + agg.timeout + agg.double_ko + agg.sudden_death).max(1) as f64;
    let ko_pct = agg.ko as f64 * 100.0 / ending_total;
    let timeout_pct = agg.timeout as f64 * 100.0 / ending_total;
    let double_ko_pct = agg.double_ko as f64 * 100.0 / ending_total;
    let sudden_death_pct = agg.sudden_death as f64 * 100.0 / ending_total;
    let mean_round_duration_ticks = if agg.rounds > 0 { agg.round_duration_ticks_sum as f64 / agg.rounds as f64 } else { 0.0 };
    let tick_rate = 60.0; // ticks per second, from the default ruleset's timing
    let mean_round_duration_seconds = mean_round_duration_ticks / tick_rate;
    let mean_damage_per_bout = if agg.bouts > 0 { agg.damage_per_bout_sum as f64 / agg.bouts as f64 } else { 0.0 };
    let guard_crushes_per_100_rounds = if agg.rounds > 0 { agg.guard_crushes as f64 * 100.0 / agg.rounds as f64 } else { 0.0 };
    let trades_per_100_rounds = if agg.rounds > 0 { agg.trades as f64 * 100.0 / agg.rounds as f64 } else { 0.0 };
    let mut distance_pct = [0.0f64; DISTANCE_BUCKETS];
    if agg.total_ticks > 0 {
        for b in 0..DISTANCE_BUCKETS {
            distance_pct[b] = agg.distance_ticks[b] as f64 * 100.0 / agg.total_ticks as f64;
        }
    }
    let far_pct = distance_pct[DISTANCE_BUCKETS - 1] + distance_pct[DISTANCE_BUCKETS - 2];

    println!("Round endings: ko={ko_pct:.1}% timeout={timeout_pct:.1}% double_ko={double_ko_pct:.1}% sudden_death={sudden_death_pct:.1}%  ({} rounds total)", agg.rounds);
    println!("Mean round duration: {mean_round_duration_ticks:.0} ticks ({mean_round_duration_seconds:.1}s)");
    println!("Mean damage per bout: {mean_damage_per_bout:.0}");
    println!("Guard Crushes: {} total ({guard_crushes_per_100_rounds:.1} per 100 rounds)", agg.guard_crushes);
    println!("Trades: {} total ({trades_per_100_rounds:.1} per 100 rounds)", agg.trades);
    println!("Passivity penalties: {}", agg.passivity_penalties);
    println!(
        "Distance histogram (% of ticks, near->far): [{}]",
        distance_pct.iter().map(|p| format!("{p:.1}")).collect::<Vec<_>>().join(", ")
    );
    println!();

    // --- warnings ---
    let mut warnings = Vec::new();
    for (name, rate) in &ranked {
        if *rate > 80.0 {
            warnings.push(format!("{name} win rate {rate:.0}% (>80% vs the field) - {name} dominates; the field is too weak against it or {name} exploits something uncontested."));
        }
        if *rate < 20.0 {
            warnings.push(format!("{name} win rate {rate:.0}% (<20% vs the field) - {name} is strictly dominated; whatever it relies on doesn't work."));
        }
    }
    if timeout_pct > 30.0 {
        warnings.push(format!("Timeouts {timeout_pct:.0}% - damage too low or round too long relative to how fast fights actually resolve."));
    }
    if agg.guard_crushes == 0 {
        warnings.push("Zero Guard Crushes across the whole tournament - either no one blocks enough to matter, or guard damage/guard_max make crushing unreachable.".to_string());
    }
    if agg.trades == 0 {
        warnings.push("Zero trades across the whole tournament - agents never contest the same window; hitstop/startup timing may make trading impossible, or agents just don't engage close enough.".to_string());
    }
    if mean_round_duration_seconds > 70.0 {
        warnings.push(format!("Mean round duration {mean_round_duration_seconds:.0}s (>70s) - damage is too low relative to vitality/round_ticks, fights drag."));
    }
    if mean_round_duration_seconds < 15.0 && mean_round_duration_seconds > 0.0 {
        warnings.push(format!("Mean round duration {mean_round_duration_seconds:.0}s (<15s) - damage is too high relative to vitality, fights end before they develop."));
    }
    if far_pct > 60.0 {
        warnings.push(format!("{far_pct:.0}% of ticks spent in the two furthest distance buckets (>60%) - agents are orbiting at max range rather than engaging; approach incentives or move range may be off."));
    }

    if warnings.is_empty() {
        println!("No warnings triggered.");
    } else {
        println!("Warnings:");
        for w in &warnings {
            println!("  - {w}");
        }
    }

    if let Some(out_path) = out {
        let mut matrix_out: BTreeMap<String, BTreeMap<String, PairCellOut>> = BTreeMap::new();
        for i in 0..n {
            let mut row = BTreeMap::new();
            for j in 0..n {
                let cell = &matrix[i][j];
                let total = repeats.max(1) as f64;
                row.insert(
                    agents[j].clone(),
                    PairCellOut { win_pct_row: cell.wins_a as f64 * 100.0 / total, win_pct_col: cell.wins_b as f64 * 100.0 / total, draw_pct: cell.draws as f64 * 100.0 / total },
                );
            }
            matrix_out.insert(agents[i].clone(), row);
        }
        let mut ending_pct = BTreeMap::new();
        ending_pct.insert("ko".to_string(), ko_pct);
        ending_pct.insert("timeout".to_string(), timeout_pct);
        ending_pct.insert("double_ko".to_string(), double_ko_pct);
        ending_pct.insert("sudden_death".to_string(), sudden_death_pct);

        let output = TourneyOutput {
            agents: agents.clone(),
            repeats,
            seed,
            matrix: matrix_out,
            win_rates: ranked.clone(),
            ending_pct,
            mean_round_duration_ticks,
            mean_round_duration_seconds,
            mean_damage_per_bout,
            guard_crushes: agg.guard_crushes,
            guard_crushes_per_100_rounds,
            trades: agg.trades,
            trades_per_100_rounds,
            passivity_penalties: agg.passivity_penalties,
            distance_histogram_pct: distance_pct,
            warnings,
        };
        let json = serde_json::to_string_pretty(&output).expect("serialize tourney output");
        fs::write(&out_path, json).unwrap_or_else(|e| panic!("failed to write {out_path}: {e}"));
        println!("\nwritten -> {out_path}");
    }
}
