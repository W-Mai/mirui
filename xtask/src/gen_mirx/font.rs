use std::path::{Path, PathBuf};

use crate::run_cmd;

use super::{Result, icu_program, probe_icu};

pub fn run(args: &[String]) -> Result {
    probe_icu()?;

    let mut ttf: Option<PathBuf> = None;
    let mut charset: Option<String> = None;
    let mut charset_file: Option<PathBuf> = None;
    let mut size: Option<String> = None;
    let mut bit_depth: Option<String> = None;
    let mut spread: Option<String> = None;
    let mut min_ppem: Option<String> = None;
    let mut max_ppem: Option<String> = None;
    let mut format: String = "sdf".to_string();
    let mut out: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        let v = || -> Result<&str> {
            args.get(i + 1)
                .map(|s| s.as_str())
                .ok_or_else(|| format!("flag {} needs a value", args[i]).into())
        };
        match args[i].as_str() {
            "--ttf" => {
                ttf = Some(PathBuf::from(v()?));
                i += 2;
            }
            "--charset" => {
                charset = Some(v()?.to_string());
                i += 2;
            }
            "--charset-file" => {
                charset_file = Some(PathBuf::from(v()?));
                i += 2;
            }
            "--size" => {
                size = Some(v()?.to_string());
                i += 2;
            }
            "--bit-depth" => {
                bit_depth = Some(v()?.to_string());
                i += 2;
            }
            "--spread" => {
                spread = Some(v()?.to_string());
                i += 2;
            }
            "--min-ppem" => {
                min_ppem = Some(v()?.to_string());
                i += 2;
            }
            "--max-ppem" => {
                max_ppem = Some(v()?.to_string());
                i += 2;
            }
            "--format" => {
                format = v()?.to_string();
                i += 2;
            }
            "--out" => {
                out = Some(PathBuf::from(v()?));
                i += 2;
            }
            other => return Err(format!("unknown flag {other}").into()),
        }
    }

    let ttf = ttf.ok_or("missing --ttf")?;
    let size = size.ok_or("missing --size")?;
    let out = out.ok_or("missing --out")?;
    let bit_depth =
        bit_depth.unwrap_or_else(|| if format == "gray" { "4" } else { "8" }.to_string());

    let out_dir = out
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&out_dir)?;

    let ttf_str = ttf.to_string_lossy().into_owned();
    let out_dir_str = out_dir.to_string_lossy().into_owned();

    let mut icu_args: Vec<String> = vec![
        "bake-font".into(),
        ttf_str,
        "--size".into(),
        size.clone(),
        "--bit-depth".into(),
        bit_depth,
        "--format".into(),
        format.clone(),
        "-O".into(),
        out_dir_str,
        "-r".into(),
    ];
    if let Some(c) = &charset {
        icu_args.push("--charset".into());
        icu_args.push(c.clone());
    }
    if let Some(p) = &charset_file {
        icu_args.push("--charset-file".into());
        icu_args.push(p.to_string_lossy().into_owned());
    }
    if let Some(s) = &spread {
        icu_args.push("--spread".into());
        icu_args.push(s.clone());
    }
    if let Some(min) = &min_ppem {
        icu_args.push("--min-ppem".into());
        icu_args.push(min.clone());
    }
    if let Some(max) = &max_ppem {
        icu_args.push("--max-ppem".into());
        icu_args.push(max.clone());
    }

    let icu_args_ref: Vec<&str> = icu_args.iter().map(|s| s.as_str()).collect();
    run_cmd(&icu_program(), &icu_args_ref)?;

    let stem = ttf
        .file_stem()
        .ok_or("--ttf has no file stem")?
        .to_string_lossy()
        .into_owned();
    let suffix = if format == "gray" { "gray" } else { "sdf" };
    let generated = out_dir.join(format!("{stem}_{suffix}_{size}.mirx"));

    if generated != out {
        std::fs::rename(&generated, &out)
            .map_err(|e| format!("rename {} → {}: {e}", generated.display(), out.display()))?;
        println!("  → renamed to {}", out.display());
    }

    Ok(())
}
