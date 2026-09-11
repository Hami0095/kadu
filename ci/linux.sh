#!/usr/bin/env bash
# Determinism CI, Linux leg. Run inside a genuine Linux container
# (rust:1-bookworm) via `docker run -v "$WORKSPACE":/work -w /work
# rust:1-bookworm bash ci/linux.sh`. Mirrors the ubuntu-latest job in
# .github/workflows/determinism.yml.
set -euo pipefail

echo "--- environment ---"
uname -a
rustc --version
cargo --version

echo "--- cargo test --workspace ---"
cargo test --workspace --locked

echo "--- no-float check ---"
cargo test -p kadu-core no_float_lint -- --nocapture

echo "--- cargo build --release ---"
cargo build --release -p kadu-cli --locked

echo "--- kadu frames --check (no move may be non-negative on block, unless whitelisted) ---"
./target/release/kadu frames --check

echo "--- kadu bench (checked against determinism/expected.toml) ---"
./target/release/kadu bench --matches 10000 --seed 1 --expect determinism/expected.toml

echo "--- kadu verify: replay corpus ---"
status=0
for f in tests/corpus/*.json; do
    echo "verifying $f"
    ./target/release/kadu verify "$f" || status=1
done
exit "$status"
