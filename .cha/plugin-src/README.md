# Repository policy plugin

`api-misuse` runs through `cargo xtask cha`. Its checked-in WASM artifact is the analyzer loaded by local and CI checks; editing Rust source alone does not update the installed plugin.

## Widget dependency boundary

`widget-dependency-boundary` rejects concrete widget dependencies from the shared infrastructure paths listed in `.cha.toml`. The rule parses Rust syntax, resolves local `use` aliases, the crate's `mirui` alias, local `extern crate self` aliases, and `crate`/`self`/`super` paths. It checks grouped imports, re-exports, type references, calls, and explicit paths inside macro and attribute tokens. Comments, strings, and `#[cfg(test)]` items are not production dependencies. Malformed protected Rust fails the check.

The general layout locator cannot depend on a particular widget type. App lifecycle and shared UI maintenance cannot invoke widget implementation helpers. Existing integration symbols have exact file-and-symbol entries; allowing one symbol does not allow its containing module. The render/layout integration in `render_system` and widget registration remain outside this boundary. A new exception requires an architectural review, not a module-wide allowlist.

The check is syntactic, not a Rust compiler resolver. It does not expand macros or follow arbitrary re-exports through other files. Imports from the author `prelude` are rejected in protected infrastructure, so its glob exports cannot hide widget dependencies. Renaming a local import or spelling a module relatively does not bypass the check. Generic protocols, entity IDs, and caller-supplied requirements remain valid dependencies.

## Validation and installation

From `.cha/plugin-src`, run `cargo test --lib` and `cargo clippy --lib -- -D warnings`, then `cha plugin build`. From the repository root, run `cha plugin install .cha/plugin-src/api_misuse.wasm`, then `cargo xtask cha`. Commit the source and `.cha/plugins/api_misuse.wasm` together.

The policy command checks both a forbidden raw fixed-point use and an aliased widget implementation dependency before analyzing the repository. A stale WASM artifact without the boundary rule fails this health check.
