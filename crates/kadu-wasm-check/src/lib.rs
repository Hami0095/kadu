//! Not part of v0 proper. A throwaway cross-target determinism check:
//! compiles kadu-core + kadu-agent to wasm32-unknown-unknown (a different
//! codegen backend/target than the native x86_64 build) and runs the exact
//! same bench loop as `kadu bench`, exposing only the final aggregate hash.
//! If this disagrees with the native run, kadu-core is not actually
//! deterministic across targets and the wasm claim in the build prompt is
//! false. `#[no_mangle]` is the only non-default-safe bit here, needed to
//! give the export a stable name Node can call.

use kadu_agent::{Agent, Dummy, Rusher};
use kadu_core::hash::Fnv1a;
use kadu_core::{default_ruleset, Intent, Sim};

fn run_one(seed: u64) -> u64 {
    let ruleset = default_ruleset();
    let reflex_interval = ruleset.reflex_interval;
    let mut sim = Sim::new(ruleset, seed);
    let mut agent_a = Rusher::new();
    let mut agent_b = Dummy;
    let mut last_intents = [Intent::None, Intent::None];
    loop {
        let tick = sim.global_tick();
        if tick % reflex_interval == 0 {
            let obs_a = sim.observation(0);
            let obs_b = sim.observation(1);
            last_intents = [agent_a.decide(&obs_a), agent_b.decide(&obs_b)];
        }
        sim.tick(last_intents);
        if sim.is_over() {
            break;
        }
        if sim.global_tick() > 2_000_000 {
            break;
        }
    }
    sim.chain_hash()
}

/// Mirrors kadu-cli's `bench` aggregate exactly: seeds `seed..seed+matches`,
/// folds each match's final chain hash into one FNV-1a aggregate.
#[unsafe(no_mangle)]
pub extern "C" fn run_bench(matches: u32, seed: u64) -> u64 {
    let mut agg = Fnv1a::new();
    for m in 0..matches {
        let s = seed.wrapping_add(m as u64);
        agg.write_u64(run_one(s));
    }
    agg.finish()
}
