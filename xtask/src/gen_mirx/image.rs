use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use mirx::{
    PayloadLimits, Reader,
    image::{BufferRequirements, CoverageBudget, DecodeRequest, SurfaceMemoryPlan, SurfaceView},
};

use super::{Result, icu_program, memory, probe_icu};

#[derive(Debug, Eq, PartialEq)]
struct Options {
    input: PathBuf,
    output: PathBuf,
    format: String,
    coding: String,
    quality: Option<u8>,
    stride_alignment: u32,
    decode_request: DecodeRequest,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        let mut input = None;
        let mut output = None;
        let mut format = String::from("rgba8888");
        let mut coding = String::from("raw");
        let mut quality = None;
        let mut stride_alignment = 1;
        let mut decode = memory::DecodeArgs::default();
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
                _ if decode.parse(option, value)? => {}
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
            decode_request: decode.request()?,
        })
    }

    fn validate_generated_coding(&self, reader: Reader<'_>) -> Result {
        if self.coding == "raw" && reader.flat_image().is_some() {
            return Ok(());
        }
        let primary = reader
            .primary()
            .map_err(|error| format!("cannot resolve generated primary image: {error:?}"))?
            .ok_or("generated MIRX has no primary IMAGE")?;
        let image = primary
            .image()
            .map_err(|error| format!("generated IMAGE is invalid: {error:?}"))?
            .ok_or("generated MIRX primary is not IMAGE")?;
        if self.coding == "raw" {
            return image
                .raw()
                .ok_or_else(|| "icu produced encoded storage while RAW was requested".into())
                .map(|_| ());
        }

        let encoded = image
            .encoded()
            .ok_or("icu produced RAW storage for a requested coding profile")?;
        encoded
            .preflight(&PayloadLimits::HOST)
            .map_err(|error| format!("generated IMAGE preflight failed: {error:?}"))?;
        let record = encoded
            .codings()
            .get(0)
            .ok_or("generated IMAGE has no coding record")?;
        let expected = match self.coding.as_str() {
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
        if let Some(quality) = self.quality
            && record.params() != [quality]
        {
            return Err("icu produced a different quantized quality".into());
        }
        Ok(())
    }
}

#[derive(Debug)]
struct OutputReport {
    memory: SurfaceMemoryPlan,
    contract: memory::ContractReport,
    workspace: Option<BufferRequirements>,
    group_slots: usize,
    storage: &'static str,
}

impl OutputReport {
    fn workspace_byte_len(&self) -> usize {
        self.workspace.map_or(0, BufferRequirements::byte_len)
    }

    fn workspace_alignment(&self) -> mirx::ByteAlignment {
        self.workspace.map_or_else(
            || self.contract.request().workspace_alignment(),
            BufferRequirements::base_alignment,
        )
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
    let reader = Reader::open(&result.stdout)
        .map_err(|error| format!("icu produced invalid MIRX bytes: {error:?}"))?;
    options.validate_generated_coding(reader)?;
    let output_report = inspect_output(&result.stdout, options.decode_request)?;
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
    println!(
        "  access: {}, {} group slot{}",
        output_report.storage,
        output_report.group_slots,
        if output_report.group_slots == 1 {
            ""
        } else {
            "s"
        },
    );
    output_report.contract.print();
    println!(
        "  buffers: output {} B @ {} B, codec workspace {} B @ {} B",
        output_report.memory.byte_len(),
        output_report.memory.base_alignment(),
        output_report.workspace_byte_len(),
        output_report.workspace_alignment(),
    );
    for (index, plane) in output_report.memory.planes().enumerate() {
        println!(
            "  plane {index}: offset {} B, stride {} B, allocation {}x{}",
            plane.data_offset(),
            plane.stride(),
            plane.allocation_width(),
            plane.allocation_height(),
        );
    }
    Ok(())
}

fn inspect_output(bytes: &[u8], request: DecodeRequest) -> Result<OutputReport> {
    let reader = Reader::open(bytes)
        .map_err(|error| format!("generated IMAGE container is invalid: {error:?}"))?;
    reader
        .validate_known_payloads(&PayloadLimits::HOST)
        .map_err(|error| format!("generated IMAGE preflight failed: {error:?}"))?;
    if let Some(flat) = reader.flat_image() {
        let raw = flat
            .surface()
            .map_err(|error| format!("generated FLAT image is invalid: {error:?}"))?;
        return inspect_raw(
            raw,
            request,
            "borrowed FLAT samples or checked reconstruction",
        );
    }
    let entry = reader
        .primary()
        .map_err(|error| format!("generated IMAGE primary is invalid: {error:?}"))?
        .ok_or("generated container has no primary IMAGE chunk")?;
    let image = entry
        .image()
        .map_err(|error| format!("generated IMAGE payload is invalid: {error:?}"))?
        .ok_or("generated chunk type is not IMAGE")?;

    if let Some(raw) = image.raw() {
        return inspect_raw(
            raw,
            request,
            "borrowed RAW samples or checked reconstruction",
        );
    }

    let encoded = image.encoded().expect("IMAGE storage variant");
    let mut slots = vec![None; encoded.group_count()];
    let mut budget = CoverageBudget::new(PayloadLimits::HOST.max_raster_work());
    let groups = encoded
        .groups_into(&mut slots, &mut budget)
        .map_err(|error| format!("generated IMAGE groups are invalid: {error:?}"))?;
    let plan = groups
        .decode_plan_for(request, &PayloadLimits::HOST)
        .map_err(|error| format!("generated IMAGE decode plan failed: {error:?}"))?;
    Ok(OutputReport {
        memory: plan.memory_plan(),
        contract: memory::ContractReport::new(
            plan.request(),
            plan.input_alignment(),
            plan.input_addresses_are_aligned(),
        ),
        workspace: Some(plan.workspace_requirements()),
        group_slots: slots.len(),
        storage: "checked scalar reconstruction",
    })
}

fn inspect_raw(
    raw: SurfaceView<'_>,
    request: DecodeRequest,
    storage: &'static str,
) -> Result<OutputReport> {
    let memory = raw
        .surface()
        .memory_plan(request.requirements())
        .map_err(|error| format!("invalid output requirements: {error:?}"))?;
    let input_alignment = raw
        .planes()
        .map(|plane| plane.memory().required_alignment())
        .max()
        .unwrap_or(mirx::ByteAlignment::ONE);
    Ok(OutputReport {
        memory,
        contract: memory::ContractReport::new(
            request,
            input_alignment,
            raw.data_addresses_are_aligned(),
        ),
        workspace: None,
        group_slots: 0,
        storage,
    })
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
        assert_eq!(options.decode_request, DecodeRequest::default());
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

    #[test]
    fn parses_deployment_memory_contract() {
        use mirx::image::{CacheSync, MemoryPlacement};

        let options = Options::parse(&args(&[
            "--in",
            "source.png",
            "--out",
            "asset.mirx",
            "--coding",
            "lz4",
            "--output-align",
            "64",
            "--plane-align",
            "64",
            "--width-multiple",
            "16",
            "--height-multiple",
            "2",
            "--stride-multiple",
            "64",
            "--workspace-align",
            "128",
            "--input-memory",
            "flash",
            "--output-memory",
            "shared-noncoherent",
            "--workspace-memory",
            "shared-coherent",
        ]))
        .unwrap();
        let request = options.decode_request;
        assert_eq!(request.requirements().base_alignment(), 64);
        assert_eq!(request.requirements().plane_alignment(), 64);
        assert_eq!(request.requirements().width_multiple(), 16);
        assert_eq!(request.requirements().height_multiple(), 2);
        assert_eq!(request.requirements().stride_multiple(), 64);
        assert_eq!(request.workspace_alignment(), 128);
        assert_eq!(request.input(), MemoryPlacement::Flash);
        assert_eq!(request.output(), MemoryPlacement::SharedNoncoherent);
        assert_eq!(request.workspace(), MemoryPlacement::SharedCoherent);
        assert_eq!(request.input_sync(), CacheSync::None);
        assert_eq!(request.output_sync(), CacheSync::CleanAfterWrite);
    }

    #[test]
    fn rejects_inaccessible_deployment_memory() {
        for invalid in [
            args(&[
                "--in",
                "source.png",
                "--out",
                "asset.mirx",
                "--output-memory",
                "device",
            ]),
            args(&[
                "--in",
                "source.png",
                "--out",
                "asset.mirx",
                "--workspace-memory",
                "flash",
            ]),
            args(&[
                "--in",
                "source.png",
                "--out",
                "asset.mirx",
                "--workspace-align",
                "3",
            ]),
        ] {
            assert!(Options::parse(&invalid).is_err());
        }
    }

    #[test]
    fn reports_encoded_output_geometry_and_workspace() {
        use mirx::image::{
            ColorDescription, EncodedImageAsset, MemoryPlacement, SampleLayout, SurfaceDescriptor,
            SurfaceRequirements,
        };
        use mirx::{Document, EncodeOptions, coding::Rle};

        let surface =
            SurfaceDescriptor::new(4, 2, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let asset = EncodedImageAsset::new(surface, Rle::new().record(), &[0x87, 42])
            .with_input_alignment(mirx::ByteAlignment::new(64).unwrap());
        let mut document = Document::new();
        let id = document.push_encoded_image(&asset).unwrap();
        document.set_primary(id).unwrap();
        let bytes = document.encode(&EncodeOptions::new()).unwrap();
        let raw_options = Options::parse(&args(&[
            "--in",
            "source.png",
            "--out",
            "asset.mirx",
            "--coding",
            "raw",
        ]))
        .unwrap();
        assert!(
            raw_options
                .validate_generated_coding(Reader::open(&bytes).unwrap())
                .is_err()
        );
        let request = DecodeRequest::new(
            SurfaceRequirements::new()
                .with_base_alignment(mirx::ByteAlignment::new(64).unwrap())
                .with_width_multiple(8)
                .with_stride_multiple(64),
        )
        .with_input(MemoryPlacement::Flash)
        .with_output(MemoryPlacement::SharedNoncoherent)
        .with_workspace(MemoryPlacement::SharedCoherent)
        .with_workspace_alignment(mirx::ByteAlignment::new(128).unwrap());
        let report = inspect_output(&bytes, request).unwrap();

        assert_eq!(report.memory.byte_len(), 128);
        assert_eq!(report.memory.base_alignment(), 64);
        assert_eq!(report.memory.plane(0).unwrap().stride(), 64);
        assert_eq!(report.group_slots, 1);
        assert_eq!(report.workspace_byte_len(), 8);
        assert_eq!(report.workspace_alignment(), 128);
        assert_eq!(report.contract.input_alignment(), 64);
        assert_eq!(report.contract.request(), request);
    }

    #[test]
    fn reports_flat_storage_without_a_chunk_primary() {
        use mirx::Document;
        use mirx::image::{ColorFormat, ImageAsset, SurfaceRequirements};
        use std::borrow::Cow;

        let bytes = Document::new_flat(ImageAsset::new(
            2,
            1,
            ColorFormat::RGBA8888,
            8,
            Cow::Borrowed(&[1, 2, 3, 4, 5, 6, 7, 8]),
        ))
        .unwrap()
        .finish()
        .unwrap()
        .into_owned();
        let raw_options =
            Options::parse(&args(&["--in", "source.png", "--out", "asset.mirx"])).unwrap();
        raw_options
            .validate_generated_coding(Reader::open(&bytes).unwrap())
            .unwrap();
        let encoded_options = Options::parse(&args(&[
            "--in",
            "source.png",
            "--out",
            "asset.mirx",
            "--coding",
            "lz4",
        ]))
        .unwrap();
        assert!(
            encoded_options
                .validate_generated_coding(Reader::open(&bytes).unwrap())
                .is_err()
        );
        let request = DecodeRequest::new(
            SurfaceRequirements::new()
                .with_base_alignment(mirx::ByteAlignment::new(64).unwrap())
                .with_stride_multiple(64),
        )
        .with_workspace_alignment(mirx::ByteAlignment::new(64).unwrap());
        let report = inspect_output(&bytes, request).unwrap();

        assert_eq!(report.memory.byte_len(), 64);
        assert_eq!(report.memory.base_alignment(), 64);
        assert_eq!(report.memory.plane(0).unwrap().stride(), 64);
        assert_eq!(report.group_slots, 0);
        assert_eq!(report.workspace_byte_len(), 0);
        assert_eq!(report.workspace_alignment(), 64);
        assert_eq!(report.contract.input_alignment(), 1);
    }
}
