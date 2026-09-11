# Tuning changelog

One entry per deliberate v0.2 change, in build order. Every entry's aggregate
is checked into `determinism/expected.toml` in the same commit as the entry
itself — see `determinism/README.md` for why.

Measurement command for every entry, unless noted otherwise:
`kadu tourney --agents all --repeats 200 --seed 1` (7,200 bouts).

---

## Change 1 — Start separation derived from the match seed

Ruleset: 2026.1 → 2026.2 · Aggregate: `0x4ee3a3654ea228e5` → `0xfde8d169a7049322`

**Hypothesis:** v0.1's tournament played one identical match per pairing,
`repeats` times, because nothing in the simulation consumed its own PRNG and
no agent but Random uses randomness. Deriving `start_separation` from the
match seed (uniformly within ±80 of the ruleset value — `[520, 680]` for the
shipped default of 600) gives the seed something to do, so the win matrix
should stop being uniformly 100%/0%.

Implementation note: the draw jitters *around* whatever `ruleset.start_separation`
says, rather than replacing it with a hardcoded absolute range — several test
fixtures (`turtle_gets_guard_crushed`, `spammer_combos_are_capped`, the
`double_ko`/`guard_crush` replay-corpus scenarios) deliberately override
`start_separation` to force two stationary agents into range of each other,
and need `start_separation_jitter = 0` to stay exactly reproducible. The
default ruleset keeps `start_separation_jitter = 80`, reproducing exactly the
range this change was specified with.

Also implemented: the tournament's per-pairing seed is now symmetric in
`(i, j)` — cell `(i, j)` and its mirror `(j, i)` now share the identical
underlying seed (same starting-separation draw, same everything) and differ
*only* in which agent occupies which slot, rather than being two
independently-seeded samples that happened to average out. This is "the
Codex mirror rule."

**Before (v0.1 final tournament, `--repeats 200`):**
- Win matrix: every non-Random cell exactly 100.0% or 0.0%
- Distance histogram: `[11.7, 52.9, 3.6, 3.4, 24.7, 1.5, 1.1, 1.0]`
- Round endings: ko 20.1% · timeout 54.4% · double_ko 13.6% · sudden_death 11.9%
- Guard Crushes: 57.0 / 100 rounds

**After:**
- Win matrix: cells involving Random now show real variance — random vs
  turtle 92.0%, vs runner 72.0%, vs random (mirror) 45.5%, vs dummy 99.0%,
  vs spammer 0.5%. **But every non-Random-vs-non-Random cell is still
  exactly 100.0% or 0.0%** (rusher vs turtle 100%, spammer vs rusher 100%,
  spammer vs runner 100%, etc.)
- Distance histogram: `[11.7, 28.4, 28.1, 3.4, 24.6, 1.6, 1.2, 1.0]` — bucket
  2 dropped by half, bucket 3 rose by the same amount fighters now spend
  meaningfully more time at a range that used to barely exist
- Round endings, Guard Crushes, mean damage: essentially unchanged
  (ko 20.1% · timeout 54.5% · double_ko 13.6% · sudden_death 11.9% ·
  57.0 Guard Crushes / 100 rounds)

**Verdict: partially refuted, and the refutation points directly at the next
change.**

The prediction assumed *any* pairing whose outcome wasn't already fully
determined by strategy alone would show variance once starting position
varied. That's true for Random (the only agent whose own decisions are
non-deterministic) but false for every deterministic-vs-deterministic
pairing: those agents' scripts contain no randomness of their own, so once
they make contact — which happens regardless of the exact starting distance
within a 160-unit band, just a few ticks sooner or later — the *interaction*
decides the winner, not the position. And the interaction is still won or
lost by the same mechanism `kadu frames --check` already caught: Light is
+0 on block, a true infinite block string. Spammer beats Rusher (which tries
to block Spammer's Lights) and Rusher beats Turtle (which only blocks) for
identical reasons, on every seed, regardless of where they started.

This is exactly the failure mode the build prompt's stop condition describes
("something other than starting position is fully determining outcomes") —
but it isn't a *new*, unexplained determinant to go hunting for. It's the
same one Part 1 already found and named. Change 1 did what it could: it
proved the seed now matters (Random's numbers move, the distance histogram
diversified) and it correctly left alone what it can't fix — a strategy-level
exploit doesn't become fair by moving the starting line. Change 2 is that
fix, not a new investigation.

---
