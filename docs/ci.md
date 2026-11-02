# CI checks

`cargo xtask ci` runs the complete check set. Use `cargo xtask ci --list` to list checks, or pass check names to run only those checks:

```sh
cargo xtask ci format
cargo xtask ci cha-policy cha-analysis
cargo xtask ci tests-std
cargo xtask ci contracts-std
cargo xtask ci host-clippy examples
cargo xtask ci wasm-check riscv-check linux-fb-check
```

Names are validated before execution. Unknown or duplicate names fail instead of silently selecting a different check. Each check reports elapsed wall time; subprocess and scheduling time are included.

Local checks can skip cross targets or Linux-only checks when their prerequisites are unavailable. In GitHub Actions, a selected check with a missing target or unsupported host fails instead of reporting a successful skip.

`tests-std` runs workspace library and binary unit tests, runtime integration tests and doctests. `contracts-std` runs compiler diagnostics and external-crate contract tests. The corresponding `tests-linux-fb` and `contracts-linux-fb` checks retain the Linux framebuffer feature configuration and run on Linux hosts.

Integration targets are discovered from Cargo metadata. Packages declare compiler contract targets in `[package.metadata.ci].contract-tests`; every other integration target enters the runtime group automatically. A stale declaration or ambiguous workspace target name fails the check. Adding an ordinary integration test requires no CI list update.

The groups use the same workspace feature selection as full workspace testing. Separating compiler diagnostics does not disable them or replace them with source-text assertions. Native clipboard test binaries retain their own harness behavior.

On macOS, doctest linking needs the SDL library search path as well as native Rust builds. With an Apple Silicon Homebrew installation, use `env RUSTDOCFLAGS="-L native=/opt/homebrew/lib" cargo xtask ci tests-std`.

Cha checks report timings for each health fixture, repository API policy and source analysis. The health fixtures must produce their expected errors; repository checks retain their error threshold and cache policy. Failures print the captured JSON and stderr without repeating the analysis.

GitHub Actions reads check groups from `.github/ci-matrix.json` and runs up to six groups concurrently on separate runners. Formatting, API policy, general analysis, release builds, host lint/examples, runtime tests, compiler contracts and cross-target checks have independent results. Each compiler contract and runtime suite runs with both the standard and Linux framebuffer configurations. The two Cha groups each validate the analyzer and health fixtures before analysis; `cargo xtask cha` runs both together.

Groups use distinct Rust cache keys and install their required native libraries and tools. Build and size checks share a runner, as do host Clippy and examples. Cargo commands within a group run sequentially to reuse that runner's target directory. The workflow explicitly selects the pinned Rust toolchain, with cross-target components declared for the target-check group.

Toolchain components are explicit matrix prerequisites: formatting installs `rustfmt`, while host lint and RV32 checks install `clippy`. Formatting also installs `xrune-fmt`; both analysis groups install Cha. Native build/test groups install SDL and evdev development libraries. The target-check group installs the WASM and RV32 standard-library targets. Matrix coverage checks validate these requirements and their forwarding to the toolchain installer.

The final `ci` job requires successful matrix preparation and successful completion of every group. Missing, skipped, cancelled or failed results cannot produce a successful summary. Architecture tests check that the matrix covers every registered check exactly once and execute the summary script against each result combination. The release tool continues to wait for the complete workflow on its exact commit.

Parallel jobs reduce the serial critical path but can increase aggregate runner time and cache storage. New group caches may be cold; compare complete workflow durations on comparable runs before claiming a speedup. GitHub's [matrix execution documentation](https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/run-job-variations) describes runner concurrency and failure handling.
