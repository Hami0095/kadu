# Kadu

Kadu is the engine of a competitive 2D fighting game where both fighters are
autonomous AI agents. It's the referee: it advances the world, asks each
agent what it wants to do, enforces the rules, and records what happened.
Kadu contains no AI of its own and never will.

It's headless, deterministic, and fixed-point (no floats anywhere in the
simulation). Two runs of the same match, on any machine, produce bit-identical
results — that's the whole point, and it's checked on every commit across
Linux, Windows, and `wasm32` in CI.

## Get a fight running in five commands

```
git clone https://github.com/Hami0095/kadu.git
cd kadu
cargo build --release -p kadu-cli
./target/release/kadu run --agent-a rusher --agent-b dummy --seed 42 --out match.json
./target/release/kadu watch match.json
```

The last command prints the bout tick by tick — a crude ASCII line per
tick showing both fighters' positions, states, vitality, guard, and surge,
plus hit/round/match events as they happen. That's it: you just watched two
AI agents fight.

### If step 3 fails to compile on Windows

You'll see `error: linker \`link.exe\` not found`. This machine doesn't have
the MSVC Build Tools that Rust's default Windows toolchain needs. Either:

- install the *Desktop development with C++* workload from the
  [Visual Studio Build Tools installer](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio),
  or
- use the GNU toolchain instead: install a MinGW-w64 distribution (e.g. via
  `winget install BrechtSanders.WinLibs.POSIX.UCRT`, extracted somewhere
  **without spaces in the path** — `C:\mingw64`, not
  `C:\Program Files\...`), add its `bin` directory to `PATH`, then:
  ```
  rustup toolchain install stable-x86_64-pc-windows-gnu
  set RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
  ```
  (PowerShell: `$env:RUSTUP_TOOLCHAIN = "stable-x86_64-pc-windows-gnu"`.)
  CI uses this same approach — see `ci/windows.bat`.

Linux and macOS need nothing beyond a normal Rust install (`rustup-init`,
then `rustup default stable`) — `rust-toolchain.toml` pins the exact
compiler version so everyone builds with the same one.

## What else is here

```
kadu run     --agent-a <name> --agent-b <name> --seed N [--out file.json]
             Plays a match and writes a replay (+ a .stats.json breakdown).
             --stats-only skips the replay; --ruleset lets you point at a
             different rulesets/*.toml without recompiling.

kadu watch   <replay.json> [--replay-to <tick>]
             ASCII playback, one line per tick.

kadu verify  <replay.json>
             Re-simulates a replay from its recorded intent stream and
             confirms the hash chain matches byte for byte.

kadu bench   --matches N --seed S [--expect determinism/expected.toml]
             Headless throughput + divergence check across N independent
             matches. This is the determinism gate: zero divergences,
             and (with --expect) the printed aggregate must match the
             committed value.

kadu diverge --a <replay> --b <replay>
             Bisects two replays (recorded with --features trace-hashes)
             down to the first tick their hash chains disagree, and prints
             both fighters' full state at that tick. This is how you'd
             actually debug a determinism regression.

kadu tourney --agents <all|a,b,c> --repeats N [--out file.json]
             Round-robin tournament: every scripted agent vs every other,
             N bouts per pairing, deterministic per-pairing seeds. Prints a
             win matrix, aggregate stats, and a warnings block flagging
             symptoms of a broken matchup (win rates >80%/<20%, too many
             timeouts, zero Guard Crushes, etc). This is the "is this
             actually a good fight?" instrument.
```

Available agents: `dummy` (does nothing), `rusher`, `turtle` (blocks only),
`runner` (hit-and-run), `spammer` (mashes Light), `random` (uniform-random
legal intent, seeded from its own PRNG).

## Workspace layout

```
crates/
  kadu-core        the simulation - no I/O, no clock, no randomness beyond
                    its own seeded PRNG. Compiles to wasm32-unknown-unknown.
  kadu-agent       the Agent trait + the scripted agents above
  kadu-replay      replay recording, JSON (de)serialisation, verify()
  kadu-cli         the `kadu` binary: run/verify/bench/watch/diverge/tourney
  kadu-wasm-check  a throwaway crate that compiles kadu-core+kadu-agent to
                    wasm32-unknown-unknown and runs the bench under Node -
                    proof the determinism claim holds across a genuinely
                    different codegen backend, not just a different OS
rulesets/
  2026.1.toml      every tunable constant, loaded at runtime - change a
                    number here and rerun, no recompilation needed
tests/
  corpus/          a small set of recorded replays (one per round-ending
                    type, plus a long combo and a guard crush), checked by
                    `kadu verify` in CI as a regression net
  tests/*.rs       acceptance tests (one per rule in the spec) and agent
                    behaviour tests (one per rule each scripted agent
                    exists to falsify)
determinism/
  expected.toml    the aggregate hash `kadu bench` must reproduce, and the
                    ruleset/engine version it was measured against
  README.md        the (mandatory) procedure for updating that file when a
                    change is meant to alter simulation behaviour
tuning/
  CHANGELOG.md     one entry per deliberate balance change: hypothesis,
                    before/after numbers from a full tournament, verdict
```

## CI

`.github/workflows/determinism.yml` runs the determinism gate (workspace
tests, the no-float check, a release build, `kadu bench` checked against
`determinism/expected.toml`, `kadu verify` over the replay corpus) across
Linux, Windows, and macOS on every push — currently blocked by a billing
issue on the GitHub account hosting the repo, so a local **Jenkins**
instance mirrors the same checks in the meantime (see `Jenkinsfile`):
genuine Linux (Docker), genuine Windows (native), genuine `aarch64` (Docker
under QEMU emulation), and genuine `wasm32` (compiled and run under Node) -
four independently-generated code paths all required to agree on one
64-bit number. macOS isn't run anywhere right now: no Apple hardware or
cloud Mac agent is available in this environment.

## Contributing

There's no special process yet — this is early. Read
`kadu-v0-build-prompt.md` for the original design spec if you want the
full rationale behind the tick order, the state machine, or the fixed-point
requirement. Anything you change that's meant to alter simulation
behaviour needs a `determinism/expected.toml` update in the same commit —
see `determinism/README.md` for why and how.

## License

Apache-2.0 — see `LICENSE`.
