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
        _ => {
            eprintln!(
                "usage: cargo xtask <ci|build|test|lint|size|bump <major|minor|patch>|publish [--dry-run]|release>"
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
        ("size", cmd_size),
    ] {
        println!("\n=== xtask: {name} ===");
        step()?;
    }
    println!("\n✅ All CI checks passed.");
    Ok(())
}

fn cmd_build() -> Result {
    cargo(&["build", "--release", "--workspace"])
}

fn cmd_test() -> Result {
    cargo(&["test", "--workspace"])
}

fn cmd_lint() -> Result {
    cargo(&["clippy", "--workspace", "--", "-D", "warnings"])?;
    cargo(&["fmt", "--all", "--check"])
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

fn cmd_bump(level: &str) -> Result {
    if !matches!(level, "major" | "minor" | "patch") {
        return Err("usage: cargo xtask bump <major|minor|patch>".into());
    }
    let root = project_root();
    let current = read_version(&root)?;
    let next = bump_version(&current, level)?;
    println!("  → bumping {current} → {next}");

    for toml in find_cargo_tomls(&root) {
        if rewrite_version(&toml, &next)? {
            println!("  → updated {toml}");
        }
    }
    println!("  ✅ version bumped to {next}");
    println!("  → run: git add -p && git commit -m \"🔖: bump to {next}\"");
    Ok(())
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

    let mut args = vec!["publish", "-p", "mirui", "--no-verify"];
    if dry_run {
        args.push("--dry-run");
    }
    cargo(&args)?;
    let verb = if dry_run { "dry-run" } else { "published" };
    println!("  ✅ {verb} mirui");
    Ok(())
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

    println!("\n  🎉 released {tag}!");
    Ok(())
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
