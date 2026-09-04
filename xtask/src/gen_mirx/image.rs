use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{Result, icu_program, probe_icu};

#[derive(Debug, Eq, PartialEq)]
struct Options {
    input: PathBuf,
    output: PathBuf,
    format: String,
    coding: String,
    quality: Option<u8>,
    stride_alignment: u32,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        let mut input = None;
        let mut output = None;
        let mut format = String::from("rgba8888");
        let mut coding = String::from("raw");
        let mut quality = None;
        let mut stride_alignment = 1;
        let mut cursor = 0;
        while cursor < args.len() {
            let option = args[cursor].as_str();
            let value = args
                .get(cursor + 1)
                .ok_or_else(|| format!("{option} needs a value"))?;
            match option {
                "--in" => input = Some(PathBuf::from(value)),
                "--out" => output = Some(PathBuf::from(value)),
                "--format" => format = value.to_ascii_lowercase(),
                "--coding" => coding = value.to_ascii_lowercase(),
                "--quality" => {
                    quality = Some(
                        value
                            .parse::<u8>()
                            .map_err(|_| "--quality must be between 1 and 100")?,
                    );
                }
                "--stride-align" => {
                    stride_alignment = value
                        .parse::<u32>()
                        .map_err(|_| "--stride-align must be a positive power of two")?;
                }
                _ => return Err(format!("unexpected argument: {option}").into()),
            }
            cursor += 2;
        }
        let input = input.ok_or("missing --in")?;
        let output = output.ok_or("missing --out")?;
        if !matches!(
            format.as_str(),
            "rgba8888"
                | "rgb888"
                | "rgb565"
                | "rgb565-swapped"
                | "bgra8888"
                | "xrgb8888"
                | "i1"
                | "i2"
                | "i4"
                | "i8"
        ) {
            return Err(format!("unsupported MIRX image format: {format}").into());
        }
        if !matches!(
            coding.as_str(),
            "raw" | "pixel" | "rle" | "lz4" | "frequency-reversible" | "frequency-quantized"
        ) {
            return Err(format!("unsupported MIRX coding profile: {coding}").into());
        }
        if coding == "pixel" && !matches!(format.as_str(), "rgb888" | "rgba8888") {
            return Err("pixel coding requires rgb888 or rgba8888".into());
        }
        if coding.starts_with("frequency-")
            && !matches!(format.as_str(), "rgb888" | "rgba8888" | "bgra8888" | "i8")
        {
            return Err("frequency coding requires rgb888, rgba8888, bgra8888, or i8".into());
        }
        let quality = match (coding.as_str(), quality) {
            ("frequency-quantized", None) => Some(75),
            ("frequency-quantized", Some(quality @ 1..=100)) => Some(quality),
            ("frequency-quantized", Some(_)) => {
                return Err("--quality must be between 1 and 100".into());
            }
            (_, Some(_)) => {
                return Err("--quality requires --coding frequency-quantized".into());
            }
            (_, None) => None,
        };
        if stride_alignment == 0 || !stride_alignment.is_power_of_two() {
            return Err("--stride-align must be a positive power of two".into());
        }
        if coding != "raw" && stride_alignment != 1 {
            return Err("--stride-align applies only to raw storage; decoded output alignment is a runtime requirement".into());
        }
        Ok(Self {
            input,
            output,
            format,
            coding,
            quality,
            stride_alignment,
        })
    }
}

pub fn run(args: &[String]) -> Result {
    probe_icu()?;
    let options = Options::parse(args)?;
    let stride_alignment = options.stride_alignment.to_string();
    let input = options.input.to_string_lossy().into_owned();
    let mut command = Command::new(icu_program());
    command.args([
        "convert",
        &input,
        "-F",
        "mirx",
        "-C",
        &options.format,
        "--mirx-coding",
        &options.coding,
        "--output-stride-align",
        &stride_alignment,
        "--stdout",
    ]);
    let quality = options.quality.map(|quality| quality.to_string());
    if let Some(quality) = quality.as_deref() {
        command.args(["--mirx-quality", quality]);
    }
    let result = command.output()?;
    if !result.status.success() || result.stdout.is_empty() {
        let detail = String::from_utf8_lossy(&result.stderr);
        return Err(format!("icu image conversion failed: {}", detail.trim()).into());
    }
    let reader = mirx::Reader::open(&result.stdout)
        .map_err(|error| format!("icu produced invalid MIRX bytes: {error:?}"))?;
    let primary = reader
        .primary()
        .map_err(|error| format!("cannot resolve generated primary image: {error:?}"))?
        .ok_or("generated MIRX has no primary IMAGE")?;
    let image = primary
        .image()
        .map_err(|error| format!("generated IMAGE is invalid: {error:?}"))?
        .ok_or("generated MIRX primary is not IMAGE")?;
    if options.coding != "raw" {
        let encoded = image
            .encoded()
            .ok_or("icu produced RAW storage for a requested coding profile")?;
        encoded
            .preflight(&mirx::PayloadLimits::HOST)
            .map_err(|error| format!("generated IMAGE preflight failed: {error:?}"))?;
        let record = encoded
            .codings()
            .get(0)
            .ok_or("generated IMAGE has no coding record")?;
        let expected = match options.coding.as_str() {
            "pixel" => mirx::CodingId::PIXEL,
            "rle" => mirx::CodingId::RLE,
            "lz4" => mirx::CodingId::LZ4,
            "frequency-reversible" => mirx::CodingId::FREQUENCY_REVERSIBLE,
            "frequency-quantized" => mirx::CodingId::FREQUENCY_QUANTIZED,
            _ => unreachable!("validated coding profile"),
        };
        if record.id() != expected {
            return Err(format!(
                "icu produced coding {} instead of {}",
                record.id().raw(),
                expected.raw()
            )
            .into());
        }
        if let Some(quality) = options.quality
            && record.params() != [quality]
        {
            return Err("icu produced a different quantized quality".into());
        }
    }
    write_output(&options.output, &result.stdout)?;
    match options.coding.as_str() {
        "raw" => println!(
            "generated {} ({}, raw, row alignment {})",
            options.output.display(),
            options.format,
            options.stride_alignment
        ),
        "frequency-quantized" => println!(
            "generated {} ({}, frequency-quantized, quality {})",
            options.output.display(),
            options.format,
            options.quality.expect("normalized quantized quality")
        ),
        _ => println!(
            "generated {} ({}, {})",
            options.output.display(),
            options.format,
            options.coding
        ),
    }
    Ok(())
}

fn write_output(path: &Path, bytes: &[u8]) -> Result {
    fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }

    #[test]
    fn parses_explicit_lossless_profile() {
        let options = Options::parse(&args(&[
            "--in",
            "source.png",
            "--out",
            "asset.mirx",
            "--format",
            "rgb888",
            "--coding",
            "lz4",
        ]))
        .unwrap();
        assert_eq!(options.input, PathBuf::from("source.png"));
        assert_eq!(options.output, PathBuf::from("asset.mirx"));
        assert_eq!(options.format, "rgb888");
        assert_eq!(options.coding, "lz4");
        assert_eq!(options.quality, None);
        assert_eq!(options.stride_alignment, 1);
    }

    #[test]
    fn rejects_profile_and_layout_mismatches() {
        assert_eq!(
            Options::parse(&args(&[
                "--in",
                "source.png",
                "--out",
                "asset.mirx",
                "--stride-align",
                "64",
            ]))
            .unwrap()
            .stride_alignment,
            64
        );
        assert!(
            Options::parse(&args(&[
                "--in",
                "source.png",
                "--out",
                "asset.mirx",
                "--format",
                "i4",
                "--coding",
                "pixel",
            ]))
            .is_err()
        );
        assert!(
            Options::parse(&args(&[
                "--in",
                "source.png",
                "--out",
                "asset.mirx",
                "--stride-align",
                "3",
            ]))
            .is_err()
        );
    }

    #[test]
    fn parses_frequency_quality_and_rejects_ambiguous_options() {
        let options = Options::parse(&args(&[
            "--in",
            "source.png",
            "--out",
            "asset.mirx",
            "--format",
            "bgra8888",
            "--coding",
            "frequency-quantized",
            "--quality",
            "63",
        ]))
        .unwrap();
        assert_eq!(options.quality, Some(63));

        for invalid in [
            args(&[
                "--in",
                "source.png",
                "--out",
                "asset.mirx",
                "--coding",
                "frequency-reversible",
                "--quality",
                "80",
            ]),
            args(&[
                "--in",
                "source.png",
                "--out",
                "asset.mirx",
                "--format",
                "rgb565",
                "--coding",
                "frequency-quantized",
            ]),
            args(&[
                "--in",
                "source.png",
                "--out",
                "asset.mirx",
                "--coding",
                "lz4",
                "--stride-align",
                "64",
            ]),
        ] {
            assert!(Options::parse(&invalid).is_err());
        }
    }
}
