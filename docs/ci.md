# CI checks {#ci-checks}

`cargo xtask ci` runs the complete verification pipeline—from formatting and `cha` architectural analysis to dual-configuration runtime tests (`std` + `linux-fb`), `trybuild` compile-fail contracts, and bare-metal cross targets.

<ArchFlow kind="ci-matrix" />

## Running CI checks locally {#running-ci-checks-locally}

Use `cargo xtask ci --list` to list all registered checks, or pass specific check names to run a subset:

```sh
cargo xtask ci format
cargo xtask ci cha-policy cha-analysis
cargo xtask ci tests-std
cargo xtask ci contracts-std
cargo xtask ci host-clippy examples
cargo xtask ci wasm-check riscv-check linux-fb-check
```

Names are validated before execution: unknown or duplicate check names fail immediately instead of silently falling back. Each check reports elapsed wall time (including subprocess execution and OS scheduling).

Local runs can skip cross-compilation targets or Linux-only checks when their host prerequisites are unavailable. In GitHub Actions, a selected check with a missing target or unsupported host fails hard instead of reporting a successful skip.

On macOS, doctest linking requires the SDL library search path just like native binary builds. On Apple Silicon with Homebrew, run:

```sh
env RUSTDOCFLAGS="-L native=/opt/homebrew/lib" cargo xtask ci tests-std
```

---

## Check groups and runner matrix {#check-groups-and-runner-matrix}

GitHub Actions reads `.github/ci-matrix.json` and runs up to six check groups concurrently on isolated runners. Every group uses a dedicated Rust cache key and installs only its required toolchain components (`rustfmt`, `clippy`, `xrune-fmt`, `cha`, or `wasm32` / `riscv32imc` standard-library targets).

| Matrix Group | `cargo xtask ci` Checks | Key Responsibilities & Prerequisites |
| --- | --- | --- |
| **Formatting** | `format` | Verifies `cargo fmt --check` across Rust crates and `xrune-fmt` on `.xrune` style manifests |
| **Cha Policy & Analysis** | `cha-policy`, `cha-analysis` | Validates health fixtures (which must trigger their expected errors), repository API boundaries, and static architecture rules (`cargo xtask cha` runs both) |
| **Runtime Suites (`std` & `linux-fb`)** | `tests-std`, `tests-linux-fb` | Workspace library & binary unit tests, auto-discovered runtime integration tests, and doctests under both standard and Linux framebuffer features |
| **Compiler Contracts (`std` & `linux-fb`)** | `contracts-std`, `contracts-linux-fb` | Pinned `rustc 1.97.0` `trybuild` compile-fail `.stderr` diagnostics (`[package.metadata.ci].contract-tests`) and external-crate visibility contracts |
| **Host Lint, Release & Size** | `host-clippy`, `examples`, `release`, `size` | Zero-warning host Clippy lints, Gallery/example compilation, optimized release builds, and binary footprint checks |
| **Cross Targets** | `wasm-check`, `riscv-check`, `linux-fb-check` | Verifies `wasm32-unknown-unknown`, bare-metal `riscv32imc-unknown-none-elf` (`no_std + alloc`), and Linux `fbdev`/`evdev` targets |

### Test discovery and renderer unit suites {#test-discovery-and-renderer-unit-suites}

Integration targets are discovered automatically from Cargo metadata:
- Packages list compile-fail / contract targets in `[package.metadata.ci].contract-tests`;
- Every other integration target enters the runtime test group automatically—adding a new integration test requires **zero** edits to CI configuration files;
- Renderer unit tests live under `tests/unit/render/` and retain access to private rendering types through their module declarations. Run them directly with:

```sh
cargo test --lib --features sdl-gpu,wgpu render::
```

The final `ci` summary job requires every matrix group to succeed. Any missing, skipped, cancelled, or failed group fails the summary gate.

---

## Documentation checks {#documentation-checks}

From `docs/`, install locked dependencies with `bun install --frozen-lockfile`, build the compiled Wasm playground, and run the static build and bilingual anchor parity suite:

```sh
bun install --frozen-lockfile
bun run playground
bun run build
bun run test
cargo test -p mirui-first-app
```

The GitHub Pages workflow builds the interactive Gallery and the bilingual manual on separate runners. A release snapshot appears in the top-bar version selector after `mirui-first-app` builds against the published crate and Pages validates the archive hash and Wasm bundle.
