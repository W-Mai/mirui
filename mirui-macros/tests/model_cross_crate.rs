use std::{env, path::PathBuf, process::Command};

#[test]
fn model_contracts_compile_in_external_crates() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/model-contract-producer/Cargo.toml");
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let target_dir = env::var_os("CARGO_TARGET_DIR").unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../target")
            .into_os_string()
    });
    for extra in [false, true] {
        for run_tests in [false, true] {
            let mut command = Command::new(&cargo);
            command
                .arg(if run_tests { "test" } else { "check" })
                .arg("--manifest-path")
                .arg(&manifest)
                .arg("-p")
                .arg("model-contract-consumer")
                .arg("--offline")
                .env("CARGO_TARGET_DIR", &target_dir);
            let features = match (run_tests, extra) {
                (false, false) => None,
                (false, true) => Some("extra"),
                (true, false) => Some("host-tests"),
                (true, true) => Some("host-tests,extra"),
            };
            if let Some(features) = features {
                command.arg("--features").arg(features);
            }
            let output = command.output().expect("run external model fixture");
            assert!(
                output.status.success(),
                "external model fixture failed (extra={extra}, tests={run_tests}):\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
