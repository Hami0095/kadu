// Jenkins mirror of .github/workflows/determinism.yml, for use while the
// GitHub Actions billing issue on this account is unresolved.
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
//   - "macOS (ARM64)" is NOT run here: there is no Apple hardware, VM, or
//     cloud Mac agent available in this environment, and Docker cannot
//     legally or technically run macOS containers. This stage is left in
//     as documentation of what the GitHub Actions matrix covers once
//     billing is restored, and is skipped rather than faked.

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
