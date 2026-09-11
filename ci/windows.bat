@echo off
REM Determinism CI, Windows leg. Mirrors the windows-latest job in
REM .github/workflows/determinism.yml. Requires a space-free MinGW-w64
REM install on PATH (the Windows Rust std linker on this machine has no
REM MSVC Build Tools available, and rustc's mingw-self-contained linker
REM invocation breaks on paths containing spaces).
REM
REM RUSTUP_TOOLCHAIN is set explicitly rather than relying on a per-directory
REM rustup override: those overrides are keyed to the exact checkout path,
REM which differs between a local dev checkout and a CI workspace (e.g.
REM Jenkins' C:\jenkins\home\workspace\...). Without this, rustup silently
REM falls back to the global default (MSVC), which fails with "link.exe not
REM found" since no MSVC Build Tools are installed here.
setlocal
set RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu

echo --- environment ---
rustc --version
cargo --version

echo --- cargo test --workspace ---
cargo test --workspace --locked
if errorlevel 1 exit /b 1

echo --- no-float check ---
cargo test -p kadu-core no_float_lint -- --nocapture
if errorlevel 1 exit /b 1

echo --- cargo build --release ---
cargo build --release -p kadu-cli --locked
if errorlevel 1 exit /b 1

echo --- kadu frames --check (no move may be non-negative on block, unless whitelisted) ---
target\release\kadu.exe frames --check
if errorlevel 1 exit /b 1

echo --- kadu bench (checked against determinism\expected.toml) ---
target\release\kadu.exe bench --matches 10000 --seed 1 --expect determinism\expected.toml
if errorlevel 1 exit /b 1

echo --- kadu verify: replay corpus ---
for %%f in (tests\corpus\*.json) do (
    echo verifying %%f
    target\release\kadu.exe verify "%%f"
    if errorlevel 1 exit /b 1
)

exit /b 0
