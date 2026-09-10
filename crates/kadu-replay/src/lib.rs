use kadu_core::sim::MatchEndReason;
use kadu_core::{default_ruleset, Intent, Ruleset, Sim};
use serde::{Deserialize, Serialize};

pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarningEntry {
    pub global_tick: u32,
    pub fighter_idx: usize,
    pub second_warning: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultEntry {
    pub winner: Option<u8>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replay {
    pub ruleset_hash: u64,
    pub ruleset_toml: String,
    pub engine_version: String,
    pub rng_seed: u64,
    pub agent_names: [String; 2],
    /// One `(Intent, Intent)` pair per tick, in order, starting at tick 0.
    pub intent_stream: Vec<(Intent, Intent)>,
    pub warnings: Vec<WarningEntry>,
    pub result: Option<ResultEntry>,
    pub final_state_hash_chain: u64,
    /// Every per-tick state hash, in order. Only present when this replay
    /// was recorded with the `trace-hashes` feature enabled; `kadu diverge`
    /// requires it on both inputs and errors clearly otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tick_hashes: Option<Vec<u64>>,
}

pub struct VerifyResult {
    pub ok: bool,
    pub expected_chain_hash: u64,
    pub computed_chain_hash: u64,
    pub ticks_run: u32,
}

fn reason_str(r: MatchEndReason) -> String {
    match r {
        MatchEndReason::Ko => "ko".to_string(),
        MatchEndReason::Timeout => "timeout".to_string(),
        MatchEndReason::SuddenDeath => "sudden_death".to_string(),
    }
}

/// Drives a `Sim` to completion, recording every tick's intents so the bout
/// can be reproduced exactly.
pub struct Recorder {
    pub sim: Sim,
    seed: u64,
    agent_names: [String; 2],
    ruleset_toml: String,
    intent_stream: Vec<(Intent, Intent)>,
    warnings: Vec<WarningEntry>,
    result: Option<ResultEntry>,
}

impl Recorder {
    pub fn new(ruleset: Ruleset, ruleset_toml: String, seed: u64, agent_a: &str, agent_b: &str) -> Recorder {
        let sim = Sim::new(ruleset, seed);
        Recorder {
            sim,
            seed,
            agent_names: [agent_a.to_string(), agent_b.to_string()],
            ruleset_toml,
            intent_stream: Vec::new(),
            warnings: Vec::new(),
            result: None,
        }
    }

    /// Advance one tick, recording the intents that were applied.
    pub fn tick(&mut self, intents: [Intent; 2]) -> kadu_core::TickReport {
        let global_tick = self.sim.global_tick();
        let report = self.sim.tick(intents);
        self.intent_stream.push((intents[0], intents[1]));
        for w in &report.warnings {
            self.warnings.push(WarningEntry { global_tick, fighter_idx: w.fighter_idx, second_warning: w.second_warning });
        }
        if let Some(m) = &report.match_ended {
            self.result = Some(ResultEntry { winner: m.winner, reason: reason_str(m.reason) });
        }
        report
    }

    pub fn is_over(&self) -> bool {
        self.sim.is_over()
    }

    pub fn into_replay(self) -> Replay {
        #[cfg(feature = "trace-hashes")]
        let tick_hashes = Some(self.sim.tick_hashes().to_vec());
        #[cfg(not(feature = "trace-hashes"))]
        let tick_hashes = None;

        Replay {
            ruleset_hash: self.sim.ruleset.content_hash(),
            ruleset_toml: self.ruleset_toml,
            engine_version: ENGINE_VERSION.to_string(),
            rng_seed: self.seed,
            agent_names: self.agent_names,
            intent_stream: self.intent_stream,
            warnings: self.warnings,
            result: self.result,
            final_state_hash_chain: self.sim.chain_hash(),
            tick_hashes,
        }
    }
}

pub fn to_json(replay: &Replay) -> serde_json::Result<String> {
    serde_json::to_string_pretty(replay)
}

pub fn from_json(s: &str) -> serde_json::Result<Replay> {
    serde_json::from_str(s)
}

/// Re-runs the entire bout from the recorded intent stream and confirms the
/// hash chain matches byte for byte. A replay that does not verify is a bug
/// in the engine.
pub fn verify(replay: &Replay) -> VerifyResult {
    let ruleset = Ruleset::from_toml_str(&replay.ruleset_toml).unwrap_or_else(|_| default_ruleset());
    let mut sim = Sim::new(ruleset, replay.rng_seed);
    for (a, b) in &replay.intent_stream {
        sim.tick([*a, *b]);
    }
    let computed = sim.chain_hash();
    VerifyResult {
        ok: computed == replay.final_state_hash_chain,
        expected_chain_hash: replay.final_state_hash_chain,
        computed_chain_hash: computed,
        ticks_run: sim.global_tick(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kadu_core::default_ruleset;

    #[test]
    fn empty_replay_round_trips() {
        let rs = default_ruleset();
        let rec = Recorder::new(rs, kadu_core::ruleset::DEFAULT_RULESET_TOML.to_string(), 1, "a", "b");
        let replay = rec.into_replay();
        let json = to_json(&replay).unwrap();
        let back = from_json(&json).unwrap();
        assert_eq!(back.rng_seed, replay.rng_seed);
    }

    #[test]
    fn verify_passes_for_short_recorded_bout() {
        let rs = default_ruleset();
        let mut rec = Recorder::new(rs, kadu_core::ruleset::DEFAULT_RULESET_TOML.to_string(), 42, "dummy", "dummy");
        for _ in 0..200 {
            rec.tick([Intent::None, Intent::None]);
        }
        let replay = rec.into_replay();
        let result = verify(&replay);
        assert!(result.ok, "expected {} got {}", result.expected_chain_hash, result.computed_chain_hash);
    }

    #[test]
    fn verify_fails_on_tampered_stream() {
        let rs = default_ruleset();
        let mut rec = Recorder::new(rs, kadu_core::ruleset::DEFAULT_RULESET_TOML.to_string(), 42, "dummy", "dummy");
        for _ in 0..200 {
            rec.tick([Intent::None, Intent::None]);
        }
        let mut replay = rec.into_replay();
        replay.intent_stream[150].0 = Intent::Move(kadu_core::Direction::Forward);
        let result = verify(&replay);
        assert!(!result.ok);
    }
}
