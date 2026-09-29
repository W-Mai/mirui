use std::process::Command;

mod gen_mirx;
mod mirx;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const AUXILIARY_VERSIONED_MANIFESTS: &[&str] = &[".cha/plugin-src/Cargo.toml"];

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("");

    match cmd {
        "ci" => cmd_ci(),
        "cha" => cmd_cha(),
        "build" => cmd_build(),
        "test" => cmd_test(),
        "lint" => cmd_lint(),
        "size" => cmd_size(),
        "wasm-check" => cmd_wasm_check(),
        "wasm-build" => cmd_wasm_build(),
        "web-serve" => cmd_web_serve(),
        "bump" => cmd_bump(args.get(1).map(|s| s.as_str()).unwrap_or("")),
        "publish" => cmd_publish(args.iter().any(|a| a == "--dry-run")),
        "release" => cmd_release(),
        "templates-bump" => cmd_templates_bump(),
        "size-gate" => cmd_size_gate(args.get(1).map(|s| s.as_str())),
        "gen-mirx" => gen_mirx::cmd_gen_mirx(&args[1..]),
        "mirx" => mirx::run(&args[1..]),
        _ => {
            eprintln!(
                "usage: cargo xtask <ci|cha|build|test|lint|size|wasm-check|wasm-build|web-serve|bump <major|minor|patch>|publish [--dry-run]|release|templates-bump|size-gate <binary>|gen-mirx <subcmd> ...|mirx <subcmd> ...>"
            );
            std::process::exit(1);
        }
    }
}

fn cmd_ci() -> Result {
    for (name, step) in [
        ("build", cmd_build as fn() -> Result),
        ("test", cmd_test),
        ("lint", cmd_lint),
        ("examples", cmd_examples),
        ("size", cmd_size),
        ("wasm-check", cmd_wasm_check),
        ("linux-fb-check", cmd_linux_fb_check),
        ("cha", cmd_cha),
    ] {
        println!("\n=== xtask: {name} ===");
        step()?;
    }
    println!("\n✅ All CI checks passed.");
    Ok(())
}

fn cmd_wasm_check() -> Result {
    // `wasm32-unknown-unknown` is an optional rustup target; skip
    // silently when missing so host-only CI agents stay green.
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("wasm32-unknown-unknown"))
        .unwrap_or(false);
    if !installed {
        println!("  ⏭ wasm32-unknown-unknown not installed, skipping");
        return Ok(());
    }
    cargo(&[
        "check",
        "--target",
        "wasm32-unknown-unknown",
        "--no-default-features",
        "--features",
        "web-canvas",
        "--lib",
    ])
}

fn cmd_linux_fb_check() -> Result {
    // The fbdev backend is `cfg(target_os = "linux")` only. Cross-host
    // CI agents (macOS / Windows) skip silently — only the Linux
    // runner exercises the path. Pick the host's own architecture so
    // a separate cross toolchain isn't required.
    if !cfg!(target_os = "linux") {
        println!("  ⏭ host is not linux, skipping");
        return Ok(());
    }
    cargo(&[
        "check",
        "--no-default-features",
        "--features",
        "linux-fb",
        "--lib",
    ])?;
    cargo(&[
        "check",
        "-p",
        "gallery",
        "--no-default-features",
        "--features",
        "linux-fb",
        "--example",
        "linux_fb_demo",
    ])
}

fn cmd_wasm_build() -> Result {
    let web_dir = format!("{}/gallery/web", project_root());
    let status = Command::new("trunk")
        .env("NO_COLOR", "true")
        .args(["build", "--release"])
        .current_dir(&web_dir)
        .status()
        .map_err(|e| format!("trunk not found (install: cargo install --locked trunk): {e}"))?;
    if !status.success() {
        return Err("trunk build exited non-zero".into());
    }
    println!("  → wasm bundle in {web_dir}/dist");
    println!("  → dev server: cd gallery/web && trunk serve");
    Ok(())
}

fn cmd_web_serve() -> Result {
    let web_dir = format!("{}/gallery/web", project_root());
    let status = Command::new("trunk")
        .env("NO_COLOR", "true")
        .arg("serve")
        .current_dir(&web_dir)
        .status()
        .map_err(|e| format!("trunk not found (install: cargo install --locked trunk): {e}"))?;
    if !status.success() {
        return Err("trunk serve exited non-zero".into());
    }
    Ok(())
}

fn cmd_cha() -> Result {
    const CHA_VERSION: &str = "cha 1.20.0";
    const POLICY_ROOTS: &[&str] = &[
        "src/",
        "mirx/src/",
        "mirx/tests/",
        "mirui-macros/src/",
        "tests/",
        "gallery/examples/",
        "xtask/src/",
    ];

    let version = Command::new("cha")
        .arg("--version")
        .output()
        .map_err(|e| format!("cha {CHA_VERSION} is required: {e}"))?;
    let installed = String::from_utf8_lossy(&version.stdout);
    if !version.status.success() || installed.trim() != CHA_VERSION {
        return Err(format!(
            "cha version mismatch: expected `{CHA_VERSION}`, found `{}`",
            installed.trim()
        )
        .into());
    }

    let health = Command::new("cha")
        .args([
            "analyze",
            ".cha/plugin-src/fixtures/fixed_raw.rs",
            "--plugin",
            "api-misuse",
            "--format",
            "json",
            "--fail-on",
            "error",
            "--no-cache",
        ])
        .current_dir(project_root())
        .output()?;
    let health_stdout = String::from_utf8_lossy(&health.stdout);
    let health_stderr = String::from_utf8_lossy(&health.stderr);
    if health.status.success()
        || !health_stdout.contains("fixed-raw-use")
        || !health_stderr.trim().is_empty()
    {
        return Err(format!(
            "cha api-misuse health check failed closed\nstdout:\n{health_stdout}\nstderr:\n{health_stderr}"
        )
        .into());
    }

    let mut policy = Command::new("cha");
    policy.arg("analyze").args(POLICY_ROOTS).args([
        "--plugin",
        "api-misuse",
        "--format",
        "json",
        "--fail-on",
        "error",
        "--no-cache",
    ]);
    let policy = policy.current_dir(project_root()).output()?;
    if !policy.status.success() || !policy.stderr.is_empty() {
        let _ = Command::new("cha")
            .arg("analyze")
            .args(POLICY_ROOTS)
            .args(["--plugin", "api-misuse", "--all", "--no-cache"])
            .current_dir(project_root())
            .status();
        return Err(format!(
            "cha api-misuse policy failed\n{}",
            String::from_utf8_lossy(&policy.stderr)
        )
        .into());
    }

    let general = Command::new("cha")
        .args(["analyze", "src/", "--format", "json", "--no-cache"])
        .current_dir(project_root())
        .output()?;
    if !general.status.success() || !general.stderr.is_empty() {
        let _ = Command::new("cha")
            .args(["analyze", "src/", "--all", "--no-cache"])
            .current_dir(project_root())
            .status();
        return Err(format!(
            "cha general analysis failed\n{}",
            String::from_utf8_lossy(&general.stderr)
        )
        .into());
    }

    println!("  ✓ api-misuse health check and repository policy passed");
    Ok(())
}

fn cmd_build() -> Result {
    cargo(&["build", "--release", "--workspace"])
}

fn cmd_test() -> Result {
    // mock-clock mutex is `cfg(feature = "std")`-gated.
    cargo(&["test", "--workspace", "--features", "std"])?;
    // `input_state` (PointerState) is gated on `linux-fb`/`linux-drm` +
    // `target_os = "linux"`, so its tests only build on a Linux runner.
    if cfg!(target_os = "linux") {
        cargo(&["test", "--workspace", "--features", "std,linux-fb"])?;
    }
    Ok(())
}

fn cmd_lint() -> Result {
    cargo(&[
        "clippy",
        "--workspace",
        "--all-features",
        "--",
        "-D",
        "warnings",
    ])?;
    let riscv_installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .ok()
        .is_some_and(|output| {
            String::from_utf8_lossy(&output.stdout).contains("riscv32imc-unknown-none-elf")
        });
    if riscv_installed {
        cargo(&[
            "clippy",
            "--lib",
            "--no-default-features",
            "--features",
            "gallery",
            "--target",
            "riscv32imc-unknown-none-elf",
            "--",
            "-D",
            "warnings",
        ])?;
    } else {
        println!(
            "  ⏭ riscv32imc-unknown-none-elf target not installed, no_std gallery clippy skipped"
        );
    }
    // `--all-features` enables `linux-fb` + `linux-drm`, but their
    // `cfg(target_os = "linux")` gate hides them on macOS / Windows
    // hosts — CI on Linux saw the only error. Re-run pinned to the
    // installed `x86_64-unknown-linux-gnu` target when off-Linux so
    // cross-host devs catch the same clippy lints locally.
    if !cfg!(target_os = "linux") {
        let installed = Command::new("rustup")
            .args(["target", "list", "--installed"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("x86_64-unknown-linux-gnu"))
            .unwrap_or(false);
        if installed {
            cargo(&[
                "clippy",
                "--workspace",
                "--all-features",
                "--target",
                "x86_64-unknown-linux-gnu",
                "--",
                "-D",
                "warnings",
            ])?;
        } else {
            println!(
                "  ⏭ x86_64-unknown-linux-gnu target not installed, \
                 cross-host linux clippy skipped"
            );
        }
    }
    cargo(&["fmt", "--all", "--check"])?;
    println!("  → xrune-fmt --check gallery/examples + src/gallery/demos");
    xrune_fmt_check_dir("gallery/examples")?;
    xrune_fmt_check_dir("src/gallery/demos")?;
    Ok(())
}

fn xrune_fmt_bin() -> String {
    std::env::var("XRUNE_FMT_BIN").unwrap_or_else(|_| "xrune-fmt".to_string())
}

fn xrune_fmt_check_dir(dir: &str) -> Result {
    let bin = xrune_fmt_bin();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            xrune_fmt_check_dir(path.to_str().unwrap())?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            let status = std::process::Command::new(&bin)
                .args([path.to_str().unwrap(), "--check"])
                .status()?;
            if !status.success() {
                return Err(format!("xrune-fmt check failed: {}", path.display()).into());
            }
        }
    }
    Ok(())
}

fn cmd_examples() -> Result {
    cargo(&["build", "-p", "gallery", "--examples", "--all-features"])
}

fn cmd_size() -> Result {
    println!("  → building release lib...");
    cargo(&["build", "--release", "--lib"])?;
    let root = project_root();
    let output = Command::new("find")
        .args([
            &format!("{root}/target/release/deps"),
            "-name",
            "libmirui-*.rlib",
        ])
        .output()?;
    let rlib = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .to_string();
    if rlib.is_empty() {
        println!("  ⚠ could not find rlib");
        return Ok(());
    }
    let output = Command::new("size").arg(&rlib).output()?;
    println!("{}", String::from_utf8_lossy(&output.stdout));
    println!("  rlib: {rlib}");
    let meta = std::fs::metadata(&rlib)?;
    println!("  total rlib size: {} bytes", meta.len());
    Ok(())
}

/// Hot-function size budget. Targets with tight ICache (e.g. 16 KiB
/// integral) miss every frame when a single function exceeds that
/// volume. Budgets here keep the worst offenders under the line
/// after the v0.21.x dispatch_* split.
const SIZE_BUDGETS: &[(&str, &str, &str, usize)] = &[
    ("SwRenderer", "Renderer", "draw", 5 * 1024),
    ("App", "", "run", 8 * 1024),
    ("SwRenderer", "Canvas", "blit", 9 * 1024),
    ("SwRenderer", "", "dispatch_blit_quad", 6 * 1024),
    ("SwRenderer", "", "dispatch_fill", 5 * 1024),
    ("SwRenderer", "", "dispatch_fill_quad", 4 * 1024),
    ("SwRenderer", "", "draw_transformed", 5 * 1024),
];

/// Return true when `sym` is an inherent / trait impl symbol whose
/// outer impl block matches `struct_name` and (when non-empty)
/// `trait_name`. Bare free fns (no `<...>`) match when trait_name is
/// empty and the struct_name appears in the path.
///
/// Examples (trait_name = ""):
///   `<App<X<Y>>>::run`              → struct_name "App"        ✓
///   `<SwRenderer>::dispatch_fill`   → struct_name "SwRenderer" ✓
///   `<X as Y>::z`                   → trait_name="" rejects (it's a trait impl)
///
/// Examples (trait_name = "Renderer"):
///   `<SwRenderer as Renderer>::draw`     → ✓
///   `<SwRenderer as Canvas>::blit`       → trait_name "Renderer" rejects
fn matches_outer_block(sym: &str, struct_name: &str, trait_name: &str) -> bool {
    // Find the outermost `<` that closes with `>::` somewhere later.
    if !sym.starts_with('<') {
        // Bare path like `mirui::app::run`.
        return trait_name.is_empty() && sym.contains(struct_name);
    }
    // Walk forward tracking depth. We want the position of the `>` that
    // closes the very first `<`.
    let bytes = sym.as_bytes();
    let mut depth: i32 = 0;
    let mut close_idx: Option<usize> = None;
    for (i, b) in bytes.iter().enumerate() {
        match *b {
            b'<' => depth += 1,
            b'>' => {
                depth -= 1;
                if depth == 0 {
                    close_idx = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let close_idx = match close_idx {
        Some(i) => i,
        None => return false,
    };
    // The block is sym[1..close_idx]. Check it contains struct_name.
    let block = &sym[1..close_idx];

    // Find ` as ` only at depth 1 inside the block. Walk the block
    // tracking depth; at depth 0 a literal " as " separates struct
    // from trait.
    let mut d = 0i32;
    let mut as_pos: Option<usize> = None;
    let bb = block.as_bytes();
    let mut i = 0;
    while i < bb.len() {
        match bb[i] {
            b'<' => d += 1,
            b'>' => d -= 1,
            b' ' if d == 0 && i + 4 <= bb.len() && &block[i..i + 4] == " as " => {
                as_pos = Some(i);
                break;
            }
            _ => {}
        }
        i += 1;
    }

    if trait_name.is_empty() {
        // Inherent impl: must have NO ` as ` at depth 0.
        as_pos.is_none() && block.contains(struct_name)
    } else {
        // Trait impl: must have ` as ` at depth 0; struct on left,
        // trait on right.
        let pos = match as_pos {
            Some(p) => p,
            None => return false,
        };
        let lhs = &block[..pos];
        let rhs = &block[pos + 4..];
        lhs.contains(struct_name) && rhs.contains(trait_name)
    }
}

fn cmd_size_gate(binary: Option<&str>) -> Result {
    let binary = match binary {
        Some(b) => b.to_string(),
        None => {
            return Err("usage: cargo xtask size-gate <path/to/elf-binary>\n\n\
             Pass any cross-built embedded ELF; size-gate reads symbols\n\
             with rust-nm and checks hot SwRenderer / App functions\n\
             against ICache-friendly budgets."
                .into());
        }
    };
    if !std::path::Path::new(&binary).exists() {
        return Err(format!("binary not found: {binary}").into());
    }

    println!("  → reading symbols from {binary}");
    let output = Command::new("rust-nm")
        .args(["--demangle", "--print-size", "--size-sort", "-r", &binary])
        .output()
        .map_err(|e| format!("rust-nm failed (is rustup llvm-tools installed?): {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "rust-nm exited non-zero:\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let nm_output = String::from_utf8_lossy(&output.stdout);

    let mut violations = Vec::new();
    let mut hits = Vec::new();

    for (struct_name, trait_name, fn_name, budget) in SIZE_BUDGETS {
        // Generic instantiations stick `::<T1, T2, ...>` after the
        // fn name. Strip those and check the remaining path ends in
        // `::fn_name`.
        let suffix = format!("::{fn_name}");

        let mut best: Option<(String, usize)> = None;
        for line in nm_output.lines() {
            // rust-nm format with --demangle: "addr size T <demangled>"
            let parts: Vec<&str> = line.splitn(4, ' ').collect();
            if parts.len() < 4 {
                continue;
            }
            let size = match usize::from_str_radix(parts[1], 16) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let sym = parts[3];

            // Strip trailing generic instantiation `::<...>` so we can
            // match against `::fn_name` at the end.
            let core = match sym.rfind("::<") {
                Some(idx) if sym[idx..].ends_with('>') => &sym[..idx],
                _ => sym,
            };
            if !core.ends_with(&suffix) {
                continue;
            }

            // Inherent impl `<...struct...>::fn` has no ` as ` between
            // the impl-target's outermost `<` and the matching `>::`.
            // Trait impl `<...struct as ...trait>::fn` has ` as `.
            //
            // Split: find the outermost balanced `<...>::` that ends
            // before fn_name. Then check whether ` as ` appears at
            // depth-1 of that block.
            let matches_prefix = matches_outer_block(core, struct_name, trait_name);
            if !matches_prefix {
                continue;
            }

            if best.as_ref().is_none_or(|(_, s)| size > *s) {
                best = Some((sym.to_string(), size));
            }
        }
        let label = if trait_name.is_empty() {
            format!("{struct_name}::{fn_name}")
        } else {
            format!("<{struct_name} as {trait_name}>::{fn_name}")
        };
        match best {
            Some((sym, size)) => {
                let kib = size as f64 / 1024.0;
                let budget_kib = *budget as f64 / 1024.0;
                if size > *budget {
                    violations.push(format!(
                        "  ❌ {label}\n      size {size} B ({kib:.1} KiB) > budget {budget} B ({budget_kib:.1} KiB)\n      sym: {sym}"
                    ));
                } else {
                    hits.push(format!(
                        "  ✅ {label}: {size} B ({kib:.1} KiB) ≤ {budget} B ({budget_kib:.1} KiB)"
                    ));
                }
            }
            None => {
                hits.push(format!(
                    "  ⏭  {label}: not found (skipping — only relevant if SwRenderer is used)"
                ));
            }
        }
    }

    for line in &hits {
        println!("{line}");
    }
    if !violations.is_empty() {
        println!();
        for v in &violations {
            eprintln!("{v}");
        }
        return Err(format!(
            "{} hot function(s) exceed ICache budget. \
             See .local/specs/icache-perf-budget for context.",
            violations.len()
        )
        .into());
    }

    println!(
        "\n  ✅ all {} hot functions within ICache budget",
        SIZE_BUDGETS.len()
    );
    Ok(())
}

fn cmd_bump(level: &str) -> Result {
    if !matches!(level, "major" | "minor" | "patch") {
        return Err("usage: cargo xtask bump <major|minor|patch>".into());
    }
    let root = project_root();
    let current = read_version(&root)?;
    let next = bump_version(&current, level)?;
    println!("  → bumping {current} → {next}");

    // 1. [package].version in workspace members and explicitly versioned
    //    auxiliary crates. Never recurse through local experiments or vendored
    //    source trees: those manifests own unrelated package versions.
    for toml in versioned_cargo_tomls(&root)? {
        if rewrite_version(&toml, &next)? {
            println!("  → updated {toml}");
        }
    }

    // 2. Internal dependency pins on workspace crates. Workspace path
    //    deps still need a version pin for cargo publish; the root
    //    Cargo.toml carries `mirui-macros = { version = "X.Y.Z", path = ... }`
    //    and that string has to track the bumped [package].version.
    let workspace_deps = ["mirui-macros", "mirx"];
    let cargo_toml_paths = [
        format!("{root}/Cargo.toml"),
        format!("{root}/gallery/Cargo.toml"),
    ];
    for cargo_toml in &cargo_toml_paths {
        for dep in &workspace_deps {
            if patch_dep_version(cargo_toml, dep, &next)? {
                println!("  → updated {dep} pin in {cargo_toml}");
            }
        }
    }

    // 3. User-facing docs that show `mirui = { version = "X.Y", ... }`.
    //    Pin only major.minor so the literal matches what users
    //    typically write (cargo's caret semantics pull patches
    //    automatically).
    let parts: Vec<&str> = next.split('.').collect();
    if parts.len() >= 2 {
        let next_minor = format!("{}.{}", parts[0], parts[1]);
        let doc_paths = [
            format!("{root}/README.md"),
            format!("{root}/docs/quickstart.md"),
            format!("{root}/src/lib.rs"),
        ];
        for doc in &doc_paths {
            if !std::path::Path::new(doc).exists() {
                continue;
            }
            if patch_mirui_doc_literal(doc, &next_minor)? {
                println!("  → updated mirui pin literals in {doc}");
            }
        }
    }

    println!("  ✅ version bumped to {next}");
    println!("  → run: git add -p && git commit -m \"🔖: bump to {next}\"");
    Ok(())
}

/// In `[dependencies]` (or any nested dependency table) find a line
/// like `<dep_name> = { version = "X.Y.Z", ... }` or `<dep_name> = "X.Y.Z"`
/// and rewrite the version quoted string to `next`. Path-only or
/// git-only deps without a `version = "..."` field are left alone.
/// Returns whether the file changed.
fn patch_dep_version(path: &str, dep_name: &str, next: &str) -> Result<bool> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Ok(false);
    };
    let mut changed = false;
    let updated: Vec<String> = content
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            // First whitespace-or-`=`-delimited token must be the dep
            // name to avoid misfiring on e.g. `mirui-macros-foo = ...`.
            let token_break = trimmed
                .find(|c: char| c.is_whitespace() || c == '=')
                .unwrap_or(trimmed.len());
            if &trimmed[..token_break] != dep_name {
                return line.to_string();
            }
            // Two acceptable forms after the `=`:
            //   1. `<dep> = "X.Y.Z"`                   — only quoted is the version
            //   2. `<dep> = { version = "X.Y.Z", ... }` — version inside an inline table
            // Form 3 (`<dep> = { path = "..." }` with no version) must
            // be left alone; rewriting the first quoted there would
            // clobber the path.
            let Some(eq_idx) = line.find('=') else {
                return line.to_string();
            };
            let after_eq_trim = line[eq_idx + 1..].trim_start();
            let replaced = if after_eq_trim.starts_with('"') {
                replace_first_quoted(line, next)
            } else if after_eq_trim.starts_with('{') {
                line.find("version")
                    .and_then(|kw_pos| replace_first_quoted_after(line, kw_pos, next))
            } else {
                None
            };
            match replaced {
                Some(new_line) if new_line != line => {
                    changed = true;
                    new_line
                }
                _ => line.to_string(),
            }
        })
        .collect();
    if !changed {
        return Ok(false);
    }
    std::fs::write(path, updated.join("\n") + "\n")?;
    Ok(true)
}

/// In docs (markdown or Rust source comments) find every line that
/// pins mirui via `mirui = { version = "X.Y" ... }` or `mirui = "X.Y"`
/// and rewrite the first quoted string to `next_minor`. Skips lines
/// referencing other crates (e.g. `mirui-macros = ...`).
fn patch_mirui_doc_literal(path: &str, next_minor: &str) -> Result<bool> {
    let content = std::fs::read_to_string(path)?;
    let mut changed = false;
    let updated: Vec<String> = content
        .lines()
        .map(|line| {
            // Find a `mirui` token in the line followed by `=` (toml
            // dependency form). Match either bare `mirui = ...` or
            // commented-out `//! mirui = ...` / `# mirui = ...`.
            let pos = match find_mirui_pin_position(line) {
                Some(p) => p,
                None => return line.to_string(),
            };
            // The first `"..."` after `pos` is the version literal.
            if let Some(replaced) = replace_first_quoted_after(line, pos, next_minor) {
                if replaced != line {
                    changed = true;
                }
                return replaced;
            }
            line.to_string()
        })
        .collect();
    if !changed {
        return Ok(false);
    }
    std::fs::write(path, updated.join("\n") + "\n")?;
    Ok(true)
}

/// Look for a `mirui` identifier followed (after optional whitespace) by
/// `=`, with a non-identifier character (or start of line) on its left
/// so that `mirui-macros = ...` is excluded. Returns the byte index of
/// the `m` in `mirui`.
fn find_mirui_pin_position(line: &str) -> Option<usize> {
    let mut start = 0;
    while let Some(rel) = line[start..].find("mirui") {
        let abs = start + rel;
        // Left boundary: start of line or non-ident char.
        let left_ok = abs == 0
            || !line.as_bytes()[abs - 1].is_ascii_alphanumeric()
                && line.as_bytes()[abs - 1] != b'_';
        // Right boundary: char immediately after "mirui" must be
        // whitespace or `=` (so `mirui-macros` and `miruix` are ruled out).
        let after_idx = abs + "mirui".len();
        let right_ok = line[after_idx..]
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || c == '=');
        if !(left_ok && right_ok) {
            start = abs + 1;
            continue;
        }
        // Verify the line actually pins a version: there must be `=`
        // followed by a quoted string somewhere after.
        let rest = &line[after_idx..];
        if !rest.contains('=') || !rest.contains('"') {
            start = abs + 1;
            continue;
        }
        return Some(abs);
    }
    None
}

fn replace_first_quoted_after(line: &str, from: usize, new: &str) -> Option<String> {
    let start = line[from..].find('"')? + from;
    let end = line[start + 1..].find('"')?;
    Some(format!(
        "{}\"{new}\"{}",
        &line[..start],
        &line[start + 1 + end + 1..]
    ))
}

fn cmd_publish(dry_run: bool) -> Result {
    let root = project_root();
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&root)
        .output()?;
    if !output.stdout.is_empty() {
        return Err("working tree not clean".into());
    }

    for package in ["mirx", "mirui-macros", "mirui"] {
        let mut args = vec![
            "publish",
            "-p",
            package,
            "--no-verify",
            "--registry",
            "crates-io",
        ];
        if dry_run {
            args.push("--dry-run");
        }
        match cargo_capture(&args) {
            Ok(()) => {
                let verb = if dry_run { "dry-run" } else { "published" };
                println!("  ✅ {verb} {package}");
            }
            // Re-running after a mid-release failure shouldn't crash here.
            // cargo's wording also covers patch releases that don't bump
            // mirui-macros: the unchanged version is already on the index.
            Err(e)
                if {
                    let msg = e.to_string();
                    msg.contains("already uploaded") || msg.contains("already exists")
                } =>
            {
                println!("  ⏭  {package} already on crates.io, skipping");
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn cargo_capture(args: &[&str]) -> Result {
    let output = Command::new("cargo").args(args).output()?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    eprint!("{stderr}");
    Err(stderr.into_owned().into())
}

fn cmd_release() -> Result {
    let root = project_root();
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&root)
        .output()?;
    if !output.stdout.is_empty() {
        return Err("working tree not clean".into());
    }
    let branch = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&root)
        .output()?;
    if !branch.status.success() {
        return Err("git branch --show-current failed".into());
    }
    verify_release_branch(String::from_utf8_lossy(&branch.stdout).trim())?;
    let release_head = current_git_head(&root)?;

    let version = read_version(&root)?;
    let tag = format!("v{version}");
    println!("  → releasing {tag}");

    run_cmd("git", &["push", "origin", "main"])?;

    println!("  → waiting for release workflows...");
    wait_for_release_workflows(&root, &release_head)?;
    verify_release_head(&release_head, &current_git_head(&root)?)?;

    // Read both tag states before mutating either repository.
    let local_tag_ref = format!("{tag}^{{commit}}");
    let tag_sha = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", &local_tag_ref])
        .current_dir(&root)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let remote_tag_ref = format!("refs/tags/{tag}");
    let remote_peeled_ref = format!("{remote_tag_ref}^{{}}");
    let remote_tag = Command::new("git")
        .args([
            "ls-remote",
            "--tags",
            "origin",
            &remote_tag_ref,
            &remote_peeled_ref,
        ])
        .current_dir(&root)
        .output()?;
    if !remote_tag.status.success() {
        return Err(format!(
            "git ls-remote failed: {}",
            String::from_utf8_lossy(&remote_tag.stderr).trim()
        )
        .into());
    }
    let remote_tag = parse_remote_tag_sha(&remote_tag.stdout, &remote_tag_ref, &remote_peeled_ref);
    match verify_remote_tag(&tag, &release_head, remote_tag.as_deref())? {
        RemoteTagState::Missing => {
            if !verify_local_tag(&tag, &release_head, &tag_sha)? {
                run_cmd("git", &["tag", &tag, &release_head])?;
            } else {
                println!("  → tag {tag} already at release HEAD");
            }
            run_cmd("git", &["push", "origin", &tag])?;
        }
        RemoteTagState::AtHead => {
            verify_local_tag(&tag, &release_head, &tag_sha)?;
            println!("  → tag {tag} already on origin at release HEAD, skip");
        }
    }

    // Create GitHub release with changelog content
    let notes = extract_changelog_for_version(&root, &version);
    let notes_arg = if notes.is_empty() {
        format!("Release {tag}")
    } else {
        notes
    };
    println!("  → creating GitHub release...");
    let status = Command::new("gh")
        .args([
            "release", "create", &tag, "--title", &tag, "--notes", &notes_arg,
        ])
        .current_dir(&root)
        .status()
        .map_err(|e| format!("gh release create failed: {e}"))?;
    if !status.success() {
        eprintln!("  ⚠ gh release create failed (non-fatal)");
    }

    println!("  → publishing to crates.io...");
    cmd_publish(false)?;

    println!("  → bumping mirui-templates pins...");
    if let Err(e) = cmd_templates_bump() {
        eprintln!("  ⚠ templates-bump failed (non-fatal): {e}");
    }

    println!("\n  🎉 released {tag}!");
    Ok(())
}

fn verify_release_branch(branch: &str) -> std::result::Result<(), String> {
    if branch == "main" {
        Ok(())
    } else {
        let branch = if branch.is_empty() {
            "detached HEAD"
        } else {
            branch
        };
        Err(format!(
            "release must run from main; current branch is {branch}"
        ))
    }
}

fn current_git_head(root: &str) -> Result<String> {
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()?;
    if !head.status.success() {
        return Err("git rev-parse HEAD failed".into());
    }
    Ok(String::from_utf8_lossy(&head.stdout).trim().to_string())
}

fn verify_release_head(expected: &str, current: &str) -> std::result::Result<(), String> {
    if current == expected {
        Ok(())
    } else {
        Err(format!(
            "HEAD changed during release workflow wait: expected {expected}, found {current}"
        ))
    }
}

const RELEASE_WORKFLOW_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20 * 60);
const RELEASE_WORKFLOW_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Clone, Copy)]
struct ReleaseWorkflow {
    file: &'static str,
    label: &'static str,
}

const RELEASE_WORKFLOWS: [ReleaseWorkflow; 3] = [
    ReleaseWorkflow {
        file: "ci.yml",
        label: "CI",
    },
    ReleaseWorkflow {
        file: "pages.yml",
        label: "Pages",
    },
    ReleaseWorkflow {
        file: "nuttx-check.yml",
        label: "NuttX target check",
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
struct WorkflowRun {
    status: String,
    conclusion: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct WorkflowProgress {
    complete: bool,
}

fn wait_for_release_workflows(root: &str, head: &str) -> Result {
    let start = std::time::Instant::now();
    let mut progress = vec![WorkflowProgress::default(); RELEASE_WORKFLOWS.len()];

    loop {
        let observations = RELEASE_WORKFLOWS
            .iter()
            .map(|workflow| query_workflow_run(root, head, workflow.file))
            .collect::<Result<Vec<_>>>()?;
        let was_complete: Vec<bool> = progress.iter().map(|item| item.complete).collect();
        if update_workflow_progress(&RELEASE_WORKFLOWS, &mut progress, &observations)? {
            for (index, workflow) in RELEASE_WORKFLOWS.iter().enumerate() {
                if !was_complete[index] && progress[index].complete {
                    println!("  ✅ {} passed", workflow.label);
                }
            }
            return Ok(());
        }
        for (index, workflow) in RELEASE_WORKFLOWS.iter().enumerate() {
            if !was_complete[index] && progress[index].complete {
                println!("  ✅ {} passed", workflow.label);
            } else if let Some(run) = &observations[index]
                && !progress[index].complete
            {
                println!("    {}: {}...", workflow.label, run.status);
            }
        }

        if start.elapsed() >= RELEASE_WORKFLOW_TIMEOUT {
            let pending = RELEASE_WORKFLOWS
                .iter()
                .enumerate()
                .filter(|(index, _)| !progress[*index].complete)
                .map(|(_, workflow)| workflow.label)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!("release workflow timeout (20 min): {pending}").into());
        }
        std::thread::sleep(RELEASE_WORKFLOW_POLL_INTERVAL);
    }
}

fn query_workflow_run(root: &str, head: &str, workflow: &str) -> Result<Option<WorkflowRun>> {
    let output = Command::new("gh")
        .args(workflow_run_list_args(head, workflow))
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "gh run list failed for {workflow}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    parse_workflow_run(&output.stdout)
}

fn workflow_run_list_args<'a>(head: &'a str, workflow: &'a str) -> [&'a str; 14] {
    [
        "run",
        "list",
        "--workflow",
        workflow,
        "--branch",
        "main",
        "--event",
        "push",
        "--commit",
        head,
        "--limit",
        "1",
        "--json",
        "status,conclusion",
    ]
}

fn parse_workflow_run(bytes: &[u8]) -> Result<Option<WorkflowRun>> {
    let runs: serde_json::Value = serde_json::from_slice(bytes)?;
    let Some(run) = runs.as_array().and_then(|runs| runs.first()) else {
        return Ok(None);
    };
    Ok(Some(WorkflowRun {
        status: run["status"].as_str().unwrap_or_default().to_string(),
        conclusion: run["conclusion"].as_str().unwrap_or_default().to_string(),
    }))
}

fn update_workflow_progress(
    workflows: &[ReleaseWorkflow],
    progress: &mut [WorkflowProgress],
    observations: &[Option<WorkflowRun>],
) -> std::result::Result<bool, String> {
    for (index, observation) in observations.iter().enumerate() {
        let Some(run) = observation else {
            continue;
        };
        if run.status != "completed" {
            progress[index].complete = false;
            continue;
        }
        if run.conclusion != "success" {
            return Err(format!(
                "{} failed: {}",
                workflows[index].label, run.conclusion
            ));
        }
        progress[index].complete = true;
    }

    Ok(progress.iter().all(|item| item.complete))
}

fn parse_remote_tag_sha(bytes: &[u8], direct_ref: &str, peeled_ref: &str) -> Option<String> {
    let mut direct = None;
    let mut peeled = None;
    for line in String::from_utf8_lossy(bytes).lines() {
        let mut fields = line.split_whitespace();
        let (Some(sha), Some(reference)) = (fields.next(), fields.next()) else {
            continue;
        };
        if reference == peeled_ref {
            peeled = Some(sha.to_string());
        } else if reference == direct_ref {
            direct = Some(sha.to_string());
        }
    }
    peeled.or(direct)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RemoteTagState {
    Missing,
    AtHead,
}

fn verify_remote_tag(
    tag: &str,
    head: &str,
    remote_sha: Option<&str>,
) -> std::result::Result<RemoteTagState, String> {
    match remote_sha {
        None => Ok(RemoteTagState::Missing),
        Some(remote_sha) if remote_sha == head => Ok(RemoteTagState::AtHead),
        Some(remote_sha) => Err(format!(
            "remote tag {tag} points at {remote_sha}, not HEAD {head}"
        )),
    }
}

fn verify_local_tag(tag: &str, head: &str, local_sha: &str) -> std::result::Result<bool, String> {
    if local_sha.is_empty() {
        Ok(false)
    } else if local_sha == head {
        Ok(true)
    } else {
        Err(format!(
            "local tag {tag} points at {local_sha}, not release HEAD {head}"
        ))
    }
}

// --- helpers ---

fn project_root() -> String {
    std::env::var("CARGO_MANIFEST_DIR")
        .map(|d| {
            std::path::Path::new(&d)
                .parent()
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .unwrap_or_else(|_| ".".to_string())
}

fn cargo(args: &[&str]) -> Result {
    run_cmd("cargo", args)
}

pub(crate) fn run_cmd(cmd: &str, args: &[&str]) -> Result {
    println!("  → {cmd} {}", args.join(" "));
    let status = Command::new(cmd)
        .args(args)
        .current_dir(project_root())
        .status()
        .map_err(|e| format!("failed to run {cmd}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{cmd} {} failed", args.join(" ")).into())
    }
}

/// Extract the changelog section for a specific version from CHANGELOG.md
fn extract_changelog_for_version(root: &str, version: &str) -> String {
    let path = format!("{root}/CHANGELOG.md");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return String::new();
    };
    let header = format!("## [{version}]");
    let mut lines = content.lines();
    // Find the start
    let mut collecting = false;
    let mut result = Vec::new();
    for line in &mut lines {
        if collecting {
            if line.starts_with("## [") {
                break;
            }
            result.push(line);
        } else if line.starts_with(&header) {
            collecting = true;
        }
    }
    // Trim leading/trailing empty lines
    let text = result.join("\n");
    text.trim().to_string()
}

fn read_version(root: &str) -> Result<String> {
    let content = std::fs::read_to_string(format!("{root}/Cargo.toml"))?;
    content
        .lines()
        .find(|l| l.trim().starts_with("version =") && !l.contains("workspace"))
        .and_then(|l| l.split('"').nth(1))
        .map(|s| s.to_string())
        .ok_or_else(|| "could not find version".into())
}

fn versioned_cargo_tomls(root: &str) -> Result<Vec<String>> {
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }

    let mut manifests = workspace_manifests_from_metadata(&output.stdout)?;
    for relative in AUXILIARY_VERSIONED_MANIFESTS {
        let path = std::path::Path::new(root).join(relative);
        if path.is_file() {
            manifests.push(path.to_string_lossy().into_owned());
        }
    }
    manifests.sort();
    manifests.dedup();
    Ok(manifests)
}

fn workspace_manifests_from_metadata(bytes: &[u8]) -> Result<Vec<String>> {
    let metadata: serde_json::Value = serde_json::from_slice(bytes)?;
    let members = metadata["workspace_members"]
        .as_array()
        .ok_or("cargo metadata omitted workspace_members")?;
    let packages = metadata["packages"]
        .as_array()
        .ok_or("cargo metadata omitted packages")?;

    let mut manifests = Vec::with_capacity(members.len());
    for member in members {
        let id = member
            .as_str()
            .ok_or("cargo metadata workspace member is not a string")?;
        let package = packages
            .iter()
            .find(|package| package["id"].as_str() == Some(id))
            .ok_or_else(|| format!("cargo metadata omitted workspace package {id}"))?;
        let manifest = package["manifest_path"]
            .as_str()
            .ok_or_else(|| format!("cargo metadata package {id} omitted manifest_path"))?;
        manifests.push(manifest.to_string());
    }
    manifests.sort();
    manifests.dedup();
    Ok(manifests)
}

fn rewrite_version(path: &str, next: &str) -> Result<bool> {
    let content = std::fs::read_to_string(path)?;
    let mut in_package = false;
    let updated: String = content
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed == "[package]" {
                in_package = true;
            } else if trimmed.starts_with('[') && trimmed != "[package]" {
                in_package = false;
            }
            if in_package && trimmed.starts_with("version =") && !trimmed.contains("workspace") {
                replace_semver(line, next)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    if updated == content {
        return Ok(false);
    }
    std::fs::write(path, updated)?;
    Ok(true)
}

fn replace_semver(line: &str, next: &str) -> String {
    if let Some(start) = line.find('"')
        && let Some(end) = line[start + 1..].find('"')
    {
        let before = &line[..start];
        let after = &line[start + 1 + end + 1..];
        return format!("{before}\"{next}\"{after}");
    }
    line.to_string()
}

fn bump_version(version: &str, level: &str) -> Result<String> {
    let parts: Vec<u64> = version
        .split('.')
        .map(|p| p.parse::<u64>().map_err(|e| format!("bad version: {e}")))
        .collect::<std::result::Result<_, _>>()?;
    if parts.len() != 3 {
        return Err(format!("expected x.y.z, got {version}").into());
    }
    let (major, minor, patch) = (parts[0], parts[1], parts[2]);
    Ok(match level {
        "major" => format!("{}.0.0", major + 1),
        "minor" => format!("{major}.{}.0", minor + 1),
        "patch" => format!("{major}.{minor}.{}", patch + 1),
        _ => unreachable!(),
    })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        RELEASE_WORKFLOW_TIMEOUT, RELEASE_WORKFLOWS, RemoteTagState, WorkflowRun,
        ensure_clean_worktree, parse_remote_tag_sha, parse_workflow_run, stage_changed_files,
        update_workflow_progress, verify_local_tag, verify_release_branch, verify_release_head,
        verify_remote_tag, workflow_run_list_args, workspace_manifests_from_metadata,
    };

    struct TempRepo(PathBuf);

    impl TempRepo {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "mirui-xtask-{label}-{}-{nonce}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).unwrap();
            let status = Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(&path)
                .status()
                .unwrap();
            assert!(status.success());
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn completed(conclusion: &str) -> Option<WorkflowRun> {
        Some(WorkflowRun {
            status: "completed".to_string(),
            conclusion: conclusion.to_string(),
        })
    }

    fn pending() -> Option<WorkflowRun> {
        Some(WorkflowRun {
            status: "in_progress".to_string(),
            conclusion: String::new(),
        })
    }

    #[test]
    fn bump_targets_only_workspace_members() {
        let metadata = br#"{
            "workspace_members": ["root 0.42.0", "member 0.42.0"],
            "packages": [
                {
                    "id": "root 0.42.0",
                    "manifest_path": "/repo/Cargo.toml"
                },
                {
                    "id": "member 0.42.0",
                    "manifest_path": "/repo/member/Cargo.toml"
                },
                {
                    "id": "local 9.9.9",
                    "manifest_path": "/repo/.local/vendor/Cargo.toml"
                }
            ]
        }"#;

        let manifests = workspace_manifests_from_metadata(metadata).unwrap();
        assert_eq!(manifests, ["/repo/Cargo.toml", "/repo/member/Cargo.toml"]);
    }

    #[test]
    fn release_workflow_timeout_is_twenty_minutes() {
        assert_eq!(RELEASE_WORKFLOW_TIMEOUT.as_secs(), 20 * 60);
    }

    #[test]
    fn release_waits_for_all_expected_workflows() {
        assert_eq!(
            RELEASE_WORKFLOWS.map(|workflow| workflow.file),
            ["ci.yml", "pages.yml", "nuttx-check.yml"]
        );
    }

    #[test]
    fn release_requires_main_branch() {
        assert_eq!(verify_release_branch("main"), Ok(()));
        assert_eq!(
            verify_release_branch("feature/release").unwrap_err(),
            "release must run from main; current branch is feature/release"
        );
        assert_eq!(
            verify_release_branch("").unwrap_err(),
            "release must run from main; current branch is detached HEAD"
        );
    }

    #[test]
    fn release_head_must_remain_stable_while_workflows_run() {
        assert_eq!(verify_release_head("head", "head"), Ok(()));
        assert_eq!(
            verify_release_head("head", "new-head").unwrap_err(),
            "HEAD changed during release workflow wait: expected head, found new-head"
        );
    }

    #[test]
    fn parses_workflow_run_states() {
        let run = parse_workflow_run(br#"[{"status":"completed","conclusion":"success"}]"#)
            .unwrap()
            .unwrap();
        assert_eq!(run.status, "completed");
        assert_eq!(run.conclusion, "success");
        assert_eq!(parse_workflow_run(b"[]").unwrap(), None);
    }

    #[test]
    fn workflow_query_targets_main_push_for_head() {
        assert_eq!(
            workflow_run_list_args("head-sha", "ci.yml"),
            [
                "run",
                "list",
                "--workflow",
                "ci.yml",
                "--branch",
                "main",
                "--event",
                "push",
                "--commit",
                "head-sha",
                "--limit",
                "1",
                "--json",
                "status,conclusion",
            ]
        );
    }

    #[test]
    fn missing_release_workflow_never_completes() {
        let mut progress = vec![Default::default(); RELEASE_WORKFLOWS.len()];
        let observations = vec![completed("success"), completed("success"), None];

        for _ in 0..10 {
            assert!(
                !update_workflow_progress(&RELEASE_WORKFLOWS, &mut progress, &observations)
                    .unwrap()
            );
        }
        assert!(!progress[2].complete);
    }

    #[test]
    fn all_release_workflows_must_complete() {
        let mut progress = vec![Default::default(); RELEASE_WORKFLOWS.len()];
        let pending_observations = vec![completed("success"), completed("success"), pending()];
        assert!(
            !update_workflow_progress(&RELEASE_WORKFLOWS, &mut progress, &pending_observations)
                .unwrap()
        );

        let complete_observations = vec![
            completed("success"),
            completed("success"),
            completed("success"),
        ];
        assert!(
            update_workflow_progress(&RELEASE_WORKFLOWS, &mut progress, &complete_observations)
                .unwrap()
        );
    }

    #[test]
    fn a_newer_pending_observation_resumes_waiting() {
        let mut progress = vec![Default::default(); RELEASE_WORKFLOWS.len()];
        let first_observations = vec![completed("success"), pending(), pending()];
        assert!(
            !update_workflow_progress(&RELEASE_WORKFLOWS, &mut progress, &first_observations)
                .unwrap()
        );
        assert!(progress[0].complete);

        let second_observations = vec![pending(), pending(), pending()];
        assert!(
            !update_workflow_progress(&RELEASE_WORKFLOWS, &mut progress, &second_observations)
                .unwrap()
        );
        assert!(!progress[0].complete);
    }

    #[test]
    fn failed_release_workflow_stops_the_wait() {
        let mut progress = vec![Default::default(); RELEASE_WORKFLOWS.len()];
        let observations = vec![completed("failure"), completed("success"), None];
        let error =
            update_workflow_progress(&RELEASE_WORKFLOWS, &mut progress, &observations).unwrap_err();
        assert_eq!(error, "CI failed: failure");
    }

    #[test]
    fn failed_nuttx_workflow_stops_the_wait() {
        let mut progress = vec![Default::default(); RELEASE_WORKFLOWS.len()];
        let observations = vec![
            completed("success"),
            completed("success"),
            completed("failure"),
        ];
        let error =
            update_workflow_progress(&RELEASE_WORKFLOWS, &mut progress, &observations).unwrap_err();
        assert_eq!(error, "NuttX target check failed: failure");
    }

    #[test]
    fn remote_tag_parser_prefers_peeled_commit() {
        let direct = "refs/tags/v1.2.3";
        let peeled = "refs/tags/v1.2.3^{}";
        let output = b"tag-object\trefs/tags/v1.2.3\ncommit-sha\trefs/tags/v1.2.3^{}\n";
        assert_eq!(
            parse_remote_tag_sha(output, direct, peeled).as_deref(),
            Some("commit-sha")
        );
        assert_eq!(
            parse_remote_tag_sha(b"commit-sha\trefs/tags/v1.2.3\n", direct, peeled).as_deref(),
            Some("commit-sha")
        );
        assert_eq!(parse_remote_tag_sha(b"", direct, peeled), None);
    }

    #[test]
    fn remote_tag_must_resolve_to_head() {
        assert_eq!(
            verify_remote_tag("v1.2.3", "head", None).unwrap(),
            RemoteTagState::Missing
        );
        assert_eq!(
            verify_remote_tag("v1.2.3", "head", Some("head")).unwrap(),
            RemoteTagState::AtHead
        );
        assert_eq!(
            verify_remote_tag("v1.2.3", "head", Some("other")).unwrap_err(),
            "remote tag v1.2.3 points at other, not HEAD head"
        );
    }

    #[test]
    fn local_tag_must_be_missing_or_resolve_to_release_head() {
        assert!(!verify_local_tag("v1.2.3", "head", "").unwrap());
        assert!(verify_local_tag("v1.2.3", "head", "head").unwrap());
        assert_eq!(
            verify_local_tag("v1.2.3", "head", "other").unwrap_err(),
            "local tag v1.2.3 points at other, not release HEAD head"
        );
    }

    #[test]
    fn templates_bump_rejects_a_dirty_repository() {
        let repo = TempRepo::new("dirty");
        ensure_clean_worktree(repo.path()).unwrap();
        std::fs::write(repo.path().join("unrelated.txt"), "dirty\n").unwrap();
        let error = ensure_clean_worktree(repo.path()).unwrap_err().to_string();
        assert!(error.contains("working tree not clean"));
    }

    #[test]
    fn templates_bump_stages_only_changed_template_files() {
        let repo = TempRepo::new("stage");
        let template = repo.path().join("templates/desktop/cargo-generate.toml");
        let unrelated = repo.path().join("unrelated.txt");
        std::fs::create_dir_all(template.parent().unwrap()).unwrap();
        std::fs::write(&template, "default = \"0.47\"\n").unwrap();
        std::fs::write(&unrelated, "leave unstaged\n").unwrap();

        stage_changed_files(repo.path(), &[template.to_string_lossy().into_owned()]).unwrap();

        let staged = Command::new("git")
            .args(["diff", "--cached", "--name-only"])
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert!(staged.status.success());
        assert_eq!(
            String::from_utf8(staged.stdout).unwrap().trim(),
            "templates/desktop/cargo-generate.toml"
        );
        let status = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert!(
            String::from_utf8(status.stdout)
                .unwrap()
                .contains("?? unrelated.txt")
        );
    }
}

fn cmd_templates_bump() -> Result {
    let mirui_root = project_root();
    let templates_root = std::path::Path::new(&mirui_root)
        .parent()
        .ok_or("no parent of project root")?
        .join("mirui-templates");

    if !templates_root.exists() {
        println!(
            "  ⚠ mirui-templates not found at {} — skipping",
            templates_root.display()
        );
        return Ok(());
    }
    ensure_clean_worktree(&templates_root)?;

    let templates_root_str = templates_root.to_string_lossy().to_string();
    let version = read_version(&mirui_root)?;
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() < 2 {
        return Err(format!("can't parse mirui version: {version}").into());
    }
    let minor = format!("{}.{}", parts[0], parts[1]);

    // The template Cargo.toml files reference mirui via the
    // `{{mirui-version}}` placeholder, which cargo-generate substitutes
    // at generation time. We must not rewrite those literals — bump
    // only the placeholder's default in each template's
    // cargo-generate.toml.
    let mut changed_files = Vec::new();
    for path in find_files_named(&templates_root_str, "cargo-generate.toml") {
        if patch_cargo_generate_default(&path, &minor)? {
            changed_files.push(path);
        }
    }

    if changed_files.is_empty() {
        println!("  ✓ mirui-templates already defaults to {minor}");
        return Ok(());
    }

    for f in &changed_files {
        println!("  → patched {f}");
    }

    let msg = format!("🔧(release): bump mirui-version default to {minor}");
    stage_changed_files(&templates_root, &changed_files)?;
    let status = Command::new("git")
        .args(["commit", "-m", &msg])
        .current_dir(&templates_root)
        .status()
        .map_err(|e| format!("git commit failed: {e}"))?;
    if !status.success() {
        return Err("git commit failed in mirui-templates".into());
    }

    println!("  ✓ committed in mirui-templates");
    println!("  ⚠ push manually:");
    println!("      cd {} && git push", templates_root.display());

    Ok(())
}

fn ensure_clean_worktree(root: &std::path::Path) -> Result {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("git status failed in {}: {e}", root.display()))?;
    if !output.status.success() {
        return Err(format!(
            "git status failed in {}: {}",
            root.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    if !output.stdout.is_empty() {
        return Err(format!(
            "working tree not clean in {}; refusing templates bump",
            root.display()
        )
        .into());
    }
    Ok(())
}

fn stage_changed_files(root: &std::path::Path, files: &[String]) -> Result {
    let mut command = Command::new("git");
    command.arg("add").arg("--");
    for file in files {
        let relative = std::path::Path::new(file)
            .strip_prefix(root)
            .map_err(|_| format!("changed template {} is outside {}", file, root.display()))?;
        command.arg(relative);
    }
    let status = command
        .current_dir(root)
        .status()
        .map_err(|e| format!("git add failed in {}: {e}", root.display()))?;
    if !status.success() {
        return Err("git add failed in mirui-templates".into());
    }
    Ok(())
}

fn patch_cargo_generate_default(path: &str, new_minor: &str) -> Result<bool> {
    let content = std::fs::read_to_string(path)?;
    let mut in_mirui_block = false;
    let mut changed = false;
    let updated: Vec<String> = content
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed == "[placeholders.mirui-version]" {
                in_mirui_block = true;
            } else if trimmed.starts_with('[') {
                in_mirui_block = false;
            }
            if in_mirui_block
                && trimmed.starts_with("default")
                && let Some(replaced) = replace_first_quoted(line, new_minor)
            {
                if replaced != line {
                    changed = true;
                }
                return replaced;
            }
            line.to_string()
        })
        .collect();
    if !changed {
        return Ok(false);
    }
    std::fs::write(path, updated.join("\n") + "\n")?;
    Ok(true)
}

fn replace_first_quoted(line: &str, new: &str) -> Option<String> {
    let start = line.find('"')?;
    let end = line[start + 1..].find('"')?;
    Some(format!(
        "{}\"{new}\"{}",
        &line[..start],
        &line[start + 1 + end + 1..]
    ))
}

fn find_files_named(root: &str, name: &str) -> Vec<String> {
    let mut result = Vec::new();
    fn walk(dir: &std::path::Path, name: &str, result: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let dirname = path.file_name().unwrap_or_default().to_string_lossy();
                if dirname != "target" && dirname != "node_modules" && dirname != ".git" {
                    walk(&path, name, result);
                }
            } else if path.file_name().is_some_and(|f| f == name) {
                result.push(path.to_string_lossy().into_owned());
            }
        }
    }
    walk(std::path::Path::new(root), name, &mut result);
    result.sort();
    result
}
