//! kadu-core: the simulation. No I/O, no clock, no randomness beyond the
//! seeded PRNG the simulation owns. Compiles to wasm32-unknown-unknown.
#![forbid(unsafe_code)]

pub mod combat;
pub mod fighter;
pub mod fixed;
pub mod geometry;
pub mod hash;
pub mod movement;
pub mod rng;
pub mod ruleset;
pub mod sim;
pub mod state_machine;
pub mod types;

pub use fixed::{Fixed, Vec2};
pub use ruleset::{default_ruleset, Ruleset};
pub use sim::{MatchEndReason, MatchResult, Phase, RoundOutcome, Sim, TickReport, TRACE_HASHES_ENABLED};
pub use types::{Agent, AttackKind, Direction, Facing, FighterState, FighterView, Intent, Observation};

#[cfg(test)]
mod no_float_lint {
    /// This test fails the build if `f32`/`f64` literals appear in kadu-core
    /// source. Run alongside the CI grep-based check in the workspace tests.
    #[test]
    fn source_contains_no_floats() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let src_dir = std::path::Path::new(manifest_dir).join("src");
        let mut stack = vec![src_dir];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let contents = std::fs::read_to_string(&path).unwrap();
                // Built at runtime so this check's own source doesn't trip itself.
                let banned = [['f', '3', '2'].iter().collect::<String>(), ['f', '6', '4'].iter().collect::<String>()];
                for (i, line) in contents.lines().enumerate() {
                    let stripped = line.trim_start();
                    if stripped.starts_with("//") {
                        continue;
                    }
                    assert!(
                        banned.iter().all(|b| !line.contains(b.as_str())),
                        "float type found in {}:{}: {}",
                        path.display(),
                        i + 1,
                        line
                    );
                }
            }
        }
    }
}
