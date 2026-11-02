# CI checks

`cargo xtask ci` runs the complete check set. Use `cargo xtask ci --list` to list checks, or pass check names to run only those checks:

```sh
cargo xtask ci format
cargo xtask ci tests-std
cargo xtask ci contracts-std
cargo xtask ci host-clippy examples
cargo xtask ci wasm-check riscv-check linux-fb-check
```

Names are validated before execution. Unknown or duplicate names fail instead of silently selecting a different check. Each check reports elapsed wall time; subprocess and scheduling time are included.

`tests-std` runs workspace library and binary unit tests, runtime integration tests and doctests. `contracts-std` runs compiler diagnostics and external-crate contract tests. The corresponding `tests-linux-fb` and `contracts-linux-fb` checks retain the Linux framebuffer feature configuration and run on Linux hosts.

Integration targets are discovered from Cargo metadata. Packages declare compiler contract targets in `[package.metadata.ci].contract-tests`; every other integration target enters the runtime group automatically. A stale declaration or ambiguous workspace target name fails the check. Adding an ordinary integration test requires no CI list update.

The groups use the same workspace feature selection as full workspace testing. Separating compiler diagnostics does not disable them or replace them with source-text assertions. Native clipboard test binaries retain their own harness behavior.

Cha checks report timings for each health fixture, repository API policy and source analysis. The health fixtures must produce their expected errors; repository checks retain their error threshold and cache policy. Failures print the captured JSON and stderr without repeating the analysis.
