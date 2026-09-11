// The determinism gate. This is the sole CI/CD pipeline for this repo -
// GitHub Actions is not used here; everything runs on this local Jenkins
// controller.
//
// This controller runs natively on Windows, so:
//   - the "Windows (x86_64)" stage runs directly on the controller's
//     built-in node - genuinely native, same toolchain used throughout
//     local development (rustup stable-x86_64-pc-windows-gnu + a
//     space-free MinGW-w64 install for the linker). Logic lives in
//     ci/windows.bat.
//   - the "Linux (x86_64)" stage shells out to `docker run` against the
//     official rust:1-bookworm image - a genuine Linux x86_64 container
//     (confirmed via `uname -a` inside it), not an emulation or a
//     relabeled Windows build. Logic lives in ci/linux.sh, run inside
//     the container so no fragile cmd.exe/bash nested-quoting is needed
//     in this file.
//   - "aarch64 (Linux, QEMU)" runs the same ci/linux.sh inside the same
//     rust:1-bookworm image, but with `--platform linux/arm64`. Docker
//     Desktop's WSL2 backend has binfmt/QEMU support built in - confirmed
//     with a plain `uname -a` before wiring this up - so this is a genuine
//     second CPU architecture, just an emulated one (expect 10-20x slower;
//     a ~15s native bench becomes a few minutes).
//   - "wasm32 (Node)" builds kadu-wasm-check for wasm32-unknown-unknown and
//     runs its bench under Node, asserting the aggregate against
//     determinism/expected.toml. This is the strongest of the four legs:
//     it's the only one that exercises a genuinely different code
//     generator (rustc's wasm32 backend + V8's JIT), not just a different
//     OS on the same x86_64/LLVM path as the other three.
//   - "macOS (ARM64)" is NOT run here: there is no Apple hardware, VM, or
//     cloud Mac agent available in this environment, and Docker cannot
//     legally or technically run macOS containers. This stage is left in
//     as documentation of the target matrix, and is skipped rather than
//     faked; register a real 'macos'-labeled agent and flip its `when`
//     to activate it.

pipeline {
    agent none

    stages {
        stage('Determinism matrix') {
            parallel {
                stage('Linux (x86_64, Docker)') {
                    agent { label 'built-in' }
                    steps {
                        bat 'docker run --rm -v "%WORKSPACE%":/work -w /work rust:1-bookworm bash ci/linux.sh'
                    }
                }
                stage('Windows (x86_64, native)') {
                    agent { label 'built-in' }
                    environment {
                        PATH = "C:\\mingw64\\bin;${env.PATH}"
                    }
                    steps {
                        bat 'ci\\windows.bat'
                    }
                }
                stage('Linux (aarch64, QEMU)') {
                    agent { label 'built-in' }
                    steps {
                        bat 'docker run --rm --platform linux/arm64 -v "%WORKSPACE%":/work -w /work rust:1-bookworm bash ci/linux.sh'
                    }
                }
                stage('wasm32 (Node)') {
                    agent { label 'built-in' }
                    // Even a wasm32 cross-build compiles and runs build
                    // scripts/proc-macros (serde, proc-macro2, ...) for the
                    // HOST target as part of the build, so this still needs
                    // a working host linker - same fix as the native
                    // Windows stage, and for the same reason (see
                    // ci/windows.bat's comment).
                    environment {
                        PATH = "C:\\mingw64\\bin;${env.PATH}"
                        RUSTUP_TOOLCHAIN = "stable-x86_64-pc-windows-gnu"
                    }
                    steps {
                        bat 'cargo build --release --target wasm32-unknown-unknown -p kadu-wasm-check --locked'
                        bat 'node crates\\kadu-wasm-check\\run.js 10000 1 --expect determinism\\expected.toml'
                    }
                }
                stage('macOS (ARM64) - unavailable here') {
                    // beforeAgent true makes Jenkins evaluate `when` before
                    // trying to allocate an agent - without it, this stage
                    // would hang forever queued for a 'macos' label that no
                    // node here carries. Kept as documentation of what the
                    // GitHub Actions matrix covers; register a real macOS
                    // agent and flip this to true to activate it.
                    when {
                        beforeAgent true
                        expression { return false }
                    }
                    agent { label 'macos' }
                    steps {
                        echo 'macOS agent not available in this environment.'
                    }
                }
            }
        }
    }
}
