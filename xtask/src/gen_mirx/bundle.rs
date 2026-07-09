use std::path::PathBuf;

use crate::run_cmd;

use super::{Result, probe_icu};

pub fn run(args: &[String]) -> Result {
    probe_icu()?;

    let mut inputs: Vec<PathBuf> = Vec::new();
    let mut out: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                out = Some(PathBuf::from(args.get(i + 1).ok_or("--out needs a value")?));
                i += 2;
            }
            other => {
                inputs.push(PathBuf::from(other));
                i += 1;
            }
        }
    }
    let out = out.ok_or("missing --out")?;
    if inputs.len() < 2 {
        return Err("bundle needs at least two input .mirx files".into());
    }

    let out_str = out.to_string_lossy().into_owned();
    let mut icu_args: Vec<String> = vec!["merge-fonts".into()];
    for p in &inputs {
        icu_args.push(p.to_string_lossy().into_owned());
    }
    icu_args.push("-O".into());
    icu_args.push(out_str);

    let icu_args_ref: Vec<&str> = icu_args.iter().map(|s| s.as_str()).collect();
    run_cmd("icu", &icu_args_ref)
}
