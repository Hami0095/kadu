# The determinism wall

`determinism/expected.toml` records the one number that proves Kadu is
deterministic: the aggregate chain hash of `kadu bench --matches 10000 --seed 1`
(Rusher vs Dummy, 10,000 independent matches, each match's final per-tick
hash chain folded into one FNV-1a aggregate).

CI (the `Jenkinsfile`, run on a local Jenkins instance - this repo does
not use GitHub Actions) runs this bench on genuine Linux/x86_64, genuine
Windows/x86_64, genuine Linux/aarch64 (under QEMU), and genuine wasm32
(under Node) on every build, and fails if the printed aggregate does not
match `expected_aggregate` in this file, on any of the four.

## Why this file exists

A hash mismatch between two runs of the same engine version and the same
ruleset means the simulation is not actually deterministic — a stray
`HashMap` iteration order, an `f32` that crept back in, an uninitialized
field, a PRNG that isn't actually seeded from match config. That is a bug,
full stop, regardless of how the change was intended.

A hash mismatch that comes from a *deliberate* balance or engine change
(a move's damage number, a state-machine timing tweak, a new rule) is not
a bug — but it still has to be caught and acknowledged, not silently
absorbed.

This file makes both cases visible the same way: CI fails. The commit
message is what tells you which case you're in.

## Updating this file

When you make a change that is meant to alter simulation behaviour:

1. Run `kadu bench --matches 10000 --seed 1` (release build) and note the
   new `aggregate_hash`.
2. In the **same commit**:
   - Update `expected_aggregate` in `determinism/expected.toml` to the new
     value.
   - Update `ruleset_hash` if you touched `rulesets/2026.1.toml` (the
     hash is printed by `kadu run` output, or via `Ruleset::content_hash()`
     in a quick test).
   - Bump `engine_version` (and/or `ruleset` if the ruleset file's own
     version changes) to reflect the change.
   - State in the commit body **what changed and why the aggregate moved**.
     "Balance: heavy damage 100 -> 90, reduces damage-scaling test
     assumptions" is a commit body. "update expected hash" is not.

A commit that changes `expected_aggregate` without saying what changed and
why is exactly the failure mode this file exists to catch — don't be that
commit.

If CI fails and you did **not** intend to change simulation behaviour,
that is a determinism bug. Do not "fix" it by updating the expected hash.
Find the actual source of nondeterminism (see `kadu diverge`, which
bisects two traced replays down to the first differing tick).
