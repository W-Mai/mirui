use std::process::Command;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

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
        "build" => cmd_build(),
        "test" => cmd_test(),
        "lint" => cmd_lint(),
        "size" => cmd_size(),
        "bump" => cmd_bump(args.get(1).map(|s| s.as_str()).unwrap_or("")),
        "publish" => cmd_publish(args.iter().any(|a| a == "--dry-run")),
        "release" => cmd_release(),
        "templates-bump" => cmd_templates_bump(),
        "size-gate" => cmd_size_gate(args.get(1).map(|s| s.as_str())),
        _ => {
            eprintln!(
                "usage: cargo xtask <ci|build|test|lint|size|bump <major|minor|patch>|publish [--dry-run]|release|templates-bump|size-gate <binary>>"
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
        ("cha", cmd_cha),
    ] {
        println!("\n=== xtask: {name} ===");
        step()?;
    }
    println!("\n✅ All CI checks passed.");
    Ok(())
}

fn cmd_cha() -> Result {
    if Command::new("cha").arg("--version").output().is_err() {
        println!("  ⏭ cha not found, skipping");
        return Ok(());
    }
    let output = Command::new("cha")
        .args(["analyze", "src/", "--format", "json"])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let has_error = stdout.contains("\"severity\":\"error\"");
    if has_error {
        // Re-run in terminal mode so the developer sees human-readable output.
        let _ = Command::new("cha").args(["analyze", "src/"]).status();
        return Err("cha found error-level issues".into());
    }
    println!("  ✓ no error-level cha findings");
    Ok(())
}

fn cmd_build() -> Result {
    cargo(&["build", "--release", "--workspace"])
}

fn cmd_test() -> Result {
    // mock-clock mutex is `cfg(feature = "std")`-gated.
    cargo(&["test", "--workspace", "--features", "std"])
}

fn cmd_lint() -> Result {
    cargo(&[
        "+stable",
        "clippy",
        "--workspace",
        "--all-features",
        "--",
        "-D",
        "warnings",
    ])?;
    cargo(&["+stable", "fmt", "--all", "--check"])?;
    println!("  → xrune-fmt --check gallery/examples/*.rs");
    for entry in std::fs::read_dir("gallery/examples")? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "rs") {
            let status = std::process::Command::new("xrune-fmt")
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

    // 1. [package].version in every Cargo.toml under the workspace.
    for toml in find_cargo_tomls(&root) {
        if rewrite_version(&toml, &next)? {
            println!("  → updated {toml}");
        }
    }

    // 2. Internal dependency pins on workspace crates. Workspace path
    //    deps still need a version pin for cargo publish; the root
    //    Cargo.toml carries `mirui-macros = { version = "X.Y.Z", path = ... }`
    //    and that string has to track the bumped [package].version.
    let workspace_deps = ["mirui-macros"];
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

    for package in ["mirui-macros", "mirui"] {
        let mut args = vec!["publish", "-p", package, "--no-verify"];
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

    let version = read_version(&root)?;
    let tag = format!("v{version}");
    println!("  → releasing {tag}");

    run_cmd("git", &["push", "origin", "main"])?;

    // Wait for CI to pass
    println!("  → waiting for CI...");
    wait_for_ci(&root)?;

    run_cmd("git", &["tag", &tag])?;
    run_cmd("git", &["push", "origin", &tag])?;

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

fn wait_for_ci(root: &str) -> Result {
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let timeout = std::time::Duration::from_secs(10 * 60);
    let start = std::time::Instant::now();

    loop {
        std::thread::sleep(std::time::Duration::from_secs(15));

        let output = Command::new("gh")
            .args([
                "run",
                "list",
                "--workflow",
                "ci.yml",
                "--limit",
                "5",
                "--json",
                "status,conclusion,headSha",
                "-q",
                &format!(".[] | select(.headSha == \"{head}\") | [.status, .conclusion] | @tsv"),
            ])
            .current_dir(root)
            .output()?;

        let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if let Some(first) = out.lines().next() {
            let parts: Vec<&str> = first.split('\t').collect();
            let status = parts.first().copied().unwrap_or("");
            let conclusion = parts.get(1).copied().unwrap_or("");

            if status == "completed" {
                if conclusion == "success" {
                    println!("  ✅ CI passed");
                    return Ok(());
                } else {
                    return Err(format!("CI failed: {conclusion}").into());
                }
            }
            println!("    CI: {status}...");
        }

        if start.elapsed() > timeout {
            return Err("CI timeout (10 min)".into());
        }
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

fn run_cmd(cmd: &str, args: &[&str]) -> Result {
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

fn find_cargo_tomls(root: &str) -> Vec<String> {
    let mut result = Vec::new();
    fn walk(dir: &std::path::Path, result: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if name != "target" && name != "node_modules" && name != ".git" {
                    walk(&path, result);
                }
            } else if path.file_name().is_some_and(|f| f == "Cargo.toml") {
                result.push(path.to_string_lossy().into_owned());
            }
        }
    }
    walk(std::path::Path::new(root), &mut result);
    result.sort();
    result
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
    let status = Command::new("git")
        .args(["add", "."])
        .current_dir(&templates_root)
        .status()
        .map_err(|e| format!("git add failed: {e}"))?;
    if !status.success() {
        return Err("git add failed in mirui-templates".into());
    }
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
