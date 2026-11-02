use std::collections::BTreeSet;
use std::process::{Command, Output};
use std::time::Instant;

use serde_json::Value;

use super::Result;

struct Check {
    name: &'static str,
    run: fn() -> Result,
}

const CHECKS: &[Check] = &[
    Check {
        name: "format",
        run: super::cmd_format,
    },
    Check {
        name: "cha",
        run: super::cmd_cha,
    },
    Check {
        name: "build",
        run: super::cmd_build,
    },
    Check {
        name: "size",
        run: super::cmd_size,
    },
    Check {
        name: "host-clippy",
        run: super::cmd_host_clippy,
    },
    Check {
        name: "examples",
        run: super::cmd_examples,
    },
    Check {
        name: "wasm-check",
        run: super::cmd_wasm_check,
    },
    Check {
        name: "riscv-check",
        run: super::cmd_riscv_clippy,
    },
    Check {
        name: "linux-fb-check",
        run: super::cmd_linux_fb_check,
    },
    Check {
        name: "tests-std",
        run: || test_suite(false, false),
    },
    Check {
        name: "contracts-std",
        run: || test_suite(false, true),
    },
    Check {
        name: "tests-linux-fb",
        run: || test_suite(true, false),
    },
    Check {
        name: "contracts-linux-fb",
        run: || test_suite(true, true),
    },
];

fn select(args: &[String]) -> Result<Vec<&'static Check>> {
    if args.is_empty() {
        return Ok(CHECKS.iter().collect());
    }
    let mut names = BTreeSet::new();
    args.iter()
        .map(|name| {
            if !names.insert(name) {
                return Err(format!("duplicate CI check: {name}").into());
            }
            CHECKS
                .iter()
                .find(|check| check.name == name)
                .ok_or_else(|| {
                    format!("unknown CI check: {name}; use cargo xtask ci --list").into()
                })
        })
        .collect()
}

pub(super) fn run(args: &[String]) -> Result {
    if args == ["--list"] {
        for check in CHECKS {
            println!("{}", check.name);
        }
        return Ok(());
    }
    let checks = select(args)?;
    for check in checks {
        let grouped = std::env::var_os("GITHUB_ACTIONS").is_some();
        if grouped {
            println!("::group::{}", check.name);
        }
        println!("\n=== xtask: {} ===", check.name);
        let start = Instant::now();
        let result = (check.run)();
        println!(
            "=== {}: {} ({:.3}s wall) ===",
            check.name,
            if result.is_ok() { "passed" } else { "failed" },
            start.elapsed().as_secs_f64()
        );
        if grouped {
            println!("::endgroup::");
        }
        result?;
    }
    println!("\n✅ Selected CI checks passed.");
    Ok(())
}

pub(super) fn timed_output(name: &str, command: &mut Command) -> std::io::Result<Output> {
    println!("  → {name}");
    let start = Instant::now();
    let result = command.output();
    println!("  ← {name} ({:.3}s wall)", start.elapsed().as_secs_f64());
    result
}

#[derive(Debug, Default, PartialEq)]
struct TestTargets {
    runtime: BTreeSet<String>,
    contracts: BTreeSet<String>,
}

fn partition(metadata: &Value) -> Result<TestTargets> {
    let mut result = TestTargets::default();
    let mut all = BTreeSet::new();
    for package in metadata["packages"]
        .as_array()
        .ok_or("missing metadata packages")?
    {
        let targets: BTreeSet<_> = package["targets"]
            .as_array()
            .ok_or("missing package targets")?
            .iter()
            .filter(|target| {
                target["kind"]
                    .as_array()
                    .is_some_and(|kinds| kinds.iter().any(|kind| kind == "test"))
            })
            .filter_map(|target| target["name"].as_str().map(str::to_owned))
            .collect();
        let contracts: BTreeSet<_> = match package.pointer("/metadata/ci/contract-tests") {
            None => BTreeSet::new(),
            Some(value) => value
                .as_array()
                .ok_or("contract-tests must be an array")?
                .iter()
                .map(|name| {
                    name.as_str()
                        .map(str::to_owned)
                        .ok_or("contract-tests entries must be strings")
                })
                .collect::<std::result::Result<_, _>>()?,
        };
        if let Some(name) = contracts.difference(&targets).next() {
            return Err(format!("unknown contract test {name} in {}", package["name"]).into());
        }
        for name in targets {
            if !all.insert(name.clone()) {
                return Err(format!("ambiguous workspace integration test name: {name}").into());
            }
            if contracts.contains(&name) {
                result.contracts.insert(name);
            } else {
                result.runtime.insert(name);
            }
        }
    }
    Ok(result)
}

fn test_suite(linux: bool, contracts: bool) -> Result {
    if linux && !cfg!(target_os = "linux") {
        println!("  ⏭ host is not linux, Linux framebuffer tests skipped");
        return Ok(());
    }
    let metadata = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(super::project_root())
        .output()?;
    if !metadata.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&metadata.stderr)
        )
        .into());
    }
    let targets = partition(&serde_json::from_slice(&metadata.stdout)?)?;
    let features = if linux { "std,linux-fb" } else { "std" };
    let base = ["test", "--workspace", "--features", features];
    if !contracts {
        let mut args = base.to_vec();
        args.extend(["--lib", "--bins"]);
        super::cargo(&args)?;
    }
    let selected = if contracts {
        &targets.contracts
    } else {
        &targets.runtime
    };
    if !selected.is_empty() {
        let mut args = base.to_vec();
        for name in selected {
            args.extend(["--test", name]);
        }
        super::cargo(&args)?;
    }
    if !contracts {
        let mut args = base.to_vec();
        args.push("--doc");
        super::cargo(&args)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn check_selection_is_explicit_complete_and_rejects_typos_before_execution() {
        let all = select(&[]).unwrap();
        assert_eq!(all.len(), CHECKS.len());
        assert_eq!(
            all.iter().map(|c| c.name).collect::<BTreeSet<_>>().len(),
            all.len()
        );
        assert!(select(&["format".into(), "unknown".into()]).is_err());
        assert!(select(&["format".into(), "format".into()]).is_err());
        assert_eq!(
            select(&["size".into(), "build".into()])
                .unwrap()
                .iter()
                .map(|c| c.name)
                .collect::<Vec<_>>(),
            ["size", "build"]
        );
    }

    fn package(names: &[&str], contracts: &[&str]) -> Value {
        json!({"name": "fixture", "metadata": {"ci": {"contract-tests": contracts}},
            "targets": names.iter().map(|name| json!({"name": name, "kind": ["test"]})).collect::<Vec<_>>()})
    }

    #[test]
    fn new_tests_are_covered_once_without_maintaining_a_runtime_allowlist() {
        let metadata =
            json!({"packages": [package(&["runtime", "diagnostic", "new"], &["diagnostic"])]});
        let tests = partition(&metadata).unwrap();
        assert_eq!(tests.runtime, ["new".into(), "runtime".into()].into());
        assert_eq!(tests.contracts, ["diagnostic".into()].into());
        assert!(tests.runtime.is_disjoint(&tests.contracts));
    }

    #[test]
    fn stale_or_ambiguous_test_declarations_fail_closed() {
        assert!(partition(&json!({"packages": [package(&["a"], &["missing"])]})).is_err());
        assert!(
            partition(&json!({"packages": [package(&["a"], &[]), package(&["a"], &[])]})).is_err()
        );
    }

    #[test]
    fn repository_test_metadata_covers_every_integration_target() {
        let output = Command::new("cargo")
            .args(["metadata", "--no-deps", "--format-version", "1"])
            .current_dir(super::super::project_root())
            .output()
            .unwrap();
        assert!(output.status.success());
        let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
        let tests = partition(&metadata).unwrap();
        let count = metadata["packages"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|p| p["targets"].as_array().unwrap())
            .filter(|t| {
                t["kind"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|kind| kind == "test")
            })
            .count();
        assert_eq!(tests.runtime.len() + tests.contracts.len(), count);
        assert!(!tests.contracts.is_empty());
    }
}
