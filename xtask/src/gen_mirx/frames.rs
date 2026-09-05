use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use mirx::{
    ChunkFlags, ChunkType, Document, EncodeOptions, FrameEncoding, FrameEncodingSet, FramePolicy,
    FrameSequence, FrameStorage, FramesEncoder, PayloadLimits, Reader,
    image::{BufferRequirements, SurfaceDescriptor, SurfaceMemoryPlan, SurfaceRequirements},
};

use super::{Result, icu_program, probe_icu};

#[derive(Debug, Eq, PartialEq)]
struct Options {
    inputs: Vec<PathBuf>,
    output: PathBuf,
    format: String,
    timebase: u32,
    duration: u32,
    play_count: u32,
    max_delta_frames: u16,
    tiles: Option<(u32, u32)>,
    input_alignment: u32,
    output_requirements: SurfaceRequirements,
    quality: Option<u8>,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        let mut inputs = Vec::new();
        let mut output = None;
        let mut format = String::from("rgba8888");
        let mut timebase = 1_000;
        let mut duration = 40;
        let mut play_count = 0;
        let mut max_delta_frames = 8;
        let mut tiles = Some((32, 32));
        let mut input_alignment = 1u32;
        let mut output_alignment = 1u32;
        let mut plane_alignment = 1u32;
        let mut width_multiple = 1u32;
        let mut height_multiple = 1u32;
        let mut stride_multiple = 1u32;
        let mut quality = None;
        let mut cursor = 0;
        while cursor < args.len() {
            let option = args[cursor].as_str();
            let value = args
                .get(cursor + 1)
                .ok_or_else(|| format!("{option} needs a value"))?;
            match option {
                "--in" => inputs.push(PathBuf::from(value)),
                "--out" => output = Some(PathBuf::from(value)),
                "--format" => format = value.to_ascii_lowercase(),
                "--timebase" => {
                    timebase = value.parse().map_err(|_| "--timebase must be positive")?
                }
                "--duration" => {
                    duration = value.parse().map_err(|_| "--duration must be positive")?
                }
                "--play-count" => {
                    play_count = value.parse().map_err(|_| "--play-count must fit u32")?
                }
                "--max-delta-frames" => {
                    max_delta_frames = value
                        .parse()
                        .map_err(|_| "--max-delta-frames must fit u16")?
                }
                "--tile" => tiles = parse_tiles(value)?,
                "--input-align" => {
                    input_alignment = value
                        .parse()
                        .map_err(|_| "--input-align must be a positive power of two")?
                }
                "--output-align" => {
                    output_alignment = value
                        .parse()
                        .map_err(|_| "--output-align must be a positive power of two")?
                }
                "--plane-align" => {
                    plane_alignment = value
                        .parse()
                        .map_err(|_| "--plane-align must be a positive power of two")?
                }
                "--width-multiple" => {
                    width_multiple = value
                        .parse()
                        .map_err(|_| "--width-multiple must be positive")?
                }
                "--height-multiple" => {
                    height_multiple = value
                        .parse()
                        .map_err(|_| "--height-multiple must be positive")?
                }
                "--stride-multiple" => {
                    stride_multiple = value
                        .parse()
                        .map_err(|_| "--stride-multiple must be positive")?
                }
                "--quality" => {
                    quality = Some(
                        value
                            .parse()
                            .map_err(|_| "--quality must be between 1 and 100")?,
                    )
                }
                _ => return Err(format!("unexpected argument: {option}").into()),
            }
            cursor += 2;
        }
        if inputs.is_empty() {
            return Err("at least one --in frame is required".into());
        }
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
            return Err(format!("unsupported MIRX frame format: {format}").into());
        }
        if timebase == 0 {
            return Err("--timebase must be positive".into());
        }
        if duration == 0 {
            return Err("--duration must be positive".into());
        }
        if !input_alignment.is_power_of_two() {
            return Err("--input-align must be a positive power of two".into());
        }
        if !output_alignment.is_power_of_two() {
            return Err("--output-align must be a positive power of two".into());
        }
        if !plane_alignment.is_power_of_two() {
            return Err("--plane-align must be a positive power of two".into());
        }
        if width_multiple == 0 || height_multiple == 0 || stride_multiple == 0 {
            return Err("output width, height, and stride multiples must be positive".into());
        }
        if quality.is_some_and(|value| !(1..=100).contains(&value)) {
            return Err("--quality must be between 1 and 100".into());
        }
        Ok(Self {
            inputs,
            output,
            format,
            timebase,
            duration,
            play_count,
            max_delta_frames,
            tiles,
            input_alignment,
            output_requirements: SurfaceRequirements::new()
                .with_base_alignment(output_alignment)
                .with_plane_alignment(plane_alignment)
                .with_width_multiple(width_multiple)
                .with_height_multiple(height_multiple)
                .with_stride_multiple(stride_multiple),
            quality,
        })
    }
}

#[derive(Debug)]
struct DecodedFrame {
    surface: SurfaceDescriptor,
    samples: Vec<u8>,
    color_table: Option<Vec<u8>>,
}

#[derive(Debug)]
struct OutputReport {
    memory: SurfaceMemoryPlan,
    input_alignment: u32,
    group_slots: usize,
    workspace: BufferRequirements,
    backup: BufferRequirements,
}

pub fn run(args: &[String]) -> Result {
    probe_icu()?;
    let options = Options::parse(args)?;
    let mut frames = Vec::with_capacity(options.inputs.len());
    for input in &options.inputs {
        frames.push(decode_frame(input, &options.format)?);
    }
    let first = frames.first().expect("validated frame list");
    first
        .surface
        .memory_plan(options.output_requirements)
        .map_err(|error| format!("invalid output requirements: {error:?}"))?;
    for (index, frame) in frames.iter().enumerate().skip(1) {
        if frame.surface != first.surface {
            return Err(format!(
                "frame {index} has surface {:?}, expected {:?}",
                frame.surface, first.surface
            )
            .into());
        }
        if frame.color_table != first.color_table {
            return Err(format!("frame {index} uses a different indexed color table").into());
        }
    }

    let max_delta_frames = options
        .max_delta_frames
        .min(u16::try_from(frames.len().saturating_sub(1)).unwrap_or(u16::MAX));
    let sequence = FrameSequence::new(
        u32::try_from(frames.len()).map_err(|_| "too many input frames")?,
        options.timebase,
        options.duration,
    )
    .map_err(|error| format!("invalid frame sequence: {error:?}"))?
    .with_play_count(options.play_count)
    .with_max_delta_frames(max_delta_frames)
    .map_err(|error| format!("invalid frame sequence: {error:?}"))?;
    let mut profiles = FrameEncodingSet::lossless();
    if let Some(quality) = options.quality {
        profiles = profiles
            .with_quantized_frequency(quality)
            .map_err(|error| format!("invalid frequency quality: {error:?}"))?;
    }
    let mut encoder = FramesEncoder::new(sequence, first.surface)
        .map_err(|error| format!("cannot create frame encoder: {error:?}"))?
        .with_profiles(profiles)
        .map_err(|error| format!("invalid frame profiles: {error:?}"))?
        .with_policy(if options.quality.is_some() {
            FramePolicy::new(max_delta_frames).allow_lossy()
        } else {
            FramePolicy::new(max_delta_frames)
        })
        .map_err(|error| format!("invalid frame policy: {error:?}"))?
        .with_input_alignment(options.input_alignment)
        .map_err(|error| format!("invalid frame input alignment: {error:?}"))?;
    encoder = match options.tiles {
        Some((width, height)) => encoder
            .with_tiles(width, height)
            .map_err(|error| format!("invalid frame tile geometry: {error:?}"))?,
        None => encoder
            .without_tiles()
            .map_err(|error| format!("cannot disable frame tiles: {error:?}"))?,
    };
    if let Some(color_table) = &first.color_table {
        encoder = encoder
            .with_color_table(color_table)
            .map_err(|error| format!("invalid frame color table: {error:?}"))?;
    }
    for (index, frame) in frames.iter().enumerate() {
        encoder
            .push(&frame.samples)
            .map_err(|error| format!("cannot encode frame {index}: {error:?}"))?;
    }
    let encoded = encoder
        .finish()
        .map_err(|error| format!("cannot finish frame sequence: {error:?}"))?;
    let reports = encoded.reports().to_vec();
    let mut document = Document::new();
    let frames_id = document
        .push_frames_with_flags(encoded, ChunkFlags::CRITICAL)
        .map_err(|error| format!("cannot add encoded frames: {error:?}"))?;
    document
        .set_primary(frames_id)
        .map_err(|error| format!("cannot select encoded frames: {error:?}"))?;
    let bytes = document
        .encode(&EncodeOptions::new())
        .map_err(|error| format!("cannot encode FRAMES container: {error:?}"))?;
    let output_report = inspect_output(&bytes, options.output_requirements)?;
    fs::write(&options.output, &bytes)?;

    println!(
        "generated {} ({} frames, {}x{}, {}, {} bytes)",
        options.output.display(),
        frames.len(),
        first.surface.width(),
        first.surface.height(),
        options.format,
        bytes.len()
    );
    let source_bytes = frames
        .iter()
        .map(|frame| frame.samples.len())
        .sum::<usize>();
    let policy = options.quality.map_or_else(
        || String::from("lossless"),
        |quality| format!("lossy allowed, frequency quality {quality}"),
    );
    println!(
        "  storage: {} source B -> {} MIRX B ({:.3}x), policy {policy}",
        source_bytes,
        bytes.len(),
        bytes.len() as f64 / source_bytes as f64,
    );
    println!(
        "  access: recovery <= {} frames, input alignment {} B, runtime {}",
        max_delta_frames,
        output_report.input_alignment,
        runtime_path(first.surface),
    );
    println!(
        "  buffers: canvas {} B @ {} B, codec workspace {} B @ {} B, backup {} B @ {} B, group slots {}",
        output_report.memory.byte_len(),
        output_report.memory.base_alignment(),
        output_report.workspace.byte_len(),
        output_report.workspace.base_alignment(),
        output_report.backup.byte_len(),
        output_report.backup.base_alignment(),
        output_report.group_slots,
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
    for report in &reports {
        println!(
            "  frame {}: {} / {}, body {} B, stored {} B, recovery {}",
            report.frame(),
            storage_name(report.storage()),
            report.encoding().map_or("none", encoding_name),
            report.encoded_bytes(),
            report.stored_bytes(),
            report.delta_frames()
        );
    }
    Ok(())
}

fn decode_frame(path: &Path, format: &str) -> Result<DecodedFrame> {
    let input = path.to_string_lossy().into_owned();
    let result = Command::new(icu_program())
        .args([
            "convert",
            &input,
            "-F",
            "mirx",
            "-C",
            format,
            "--mirx-coding",
            "raw",
            "--output-stride-align",
            "1",
            "--stdout",
        ])
        .output()?;
    if !result.status.success() || result.stdout.is_empty() {
        let detail = String::from_utf8_lossy(&result.stderr);
        return Err(format!("icu failed to decode {}: {}", path.display(), detail.trim()).into());
    }
    let reader = Reader::open(&result.stdout)
        .map_err(|error| format!("icu produced invalid MIRX bytes: {error:?}"))?;
    let surface = if let Some(image) = reader.flat_image() {
        image
            .surface()
            .map_err(|error| format!("generated FLAT image is invalid: {error:?}"))?
    } else {
        let primary = reader
            .primary()
            .map_err(|error| format!("cannot resolve generated image: {error:?}"))?
            .ok_or("icu output has no primary IMAGE")?;
        primary
            .image()
            .map_err(|error| format!("generated IMAGE is invalid: {error:?}"))?
            .ok_or("icu output primary is not IMAGE")?
            .raw()
            .ok_or("icu returned encoded storage while raw samples were requested")?
    };
    let mut samples = Vec::with_capacity(surface.surface().tight_byte_len().unwrap_or(0) as usize);
    for plane in surface.planes() {
        for row in plane
            .rows()
            .map_err(|error| format!("cannot read generated plane rows: {error:?}"))?
        {
            samples.extend_from_slice(row);
        }
    }
    Ok(DecodedFrame {
        surface: surface.surface(),
        samples,
        color_table: surface.color_table().map(|table| table.as_bytes().to_vec()),
    })
}

fn inspect_output(bytes: &[u8], requirements: SurfaceRequirements) -> Result<OutputReport> {
    let reader = Reader::open(bytes)
        .map_err(|error| format!("generated FRAMES container is invalid: {error:?}"))?;
    reader
        .validate_known_payloads(&PayloadLimits::HOST)
        .map_err(|error| format!("generated FRAMES preflight failed: {error:?}"))?;
    let entry = reader
        .primary()
        .map_err(|error| format!("generated FRAMES primary is invalid: {error:?}"))?
        .ok_or("generated container has no primary FRAMES chunk")?;
    if entry.chunk_type() != ChunkType::FRAMES {
        return Err("generated container primary is not FRAMES".into());
    }
    let frames = entry
        .frames(&PayloadLimits::HOST)
        .map_err(|error| format!("generated FRAMES payload is invalid: {error:?}"))?
        .ok_or("generated chunk type is not FRAMES")?;
    frames
        .validate_data()
        .map_err(|error| format!("generated FRAMES DATA is invalid: {error:?}"))?;
    let mut group_slots = vec![None; frames.group_count()];
    let plan = frames
        .playback_plan(requirements, PayloadLimits::HOST, &mut group_slots)
        .map_err(|error| format!("generated FRAMES playback plan failed: {error:?}"))?;
    Ok(OutputReport {
        memory: plan.memory_plan(),
        input_alignment: frames
            .input_alignment()
            .map_err(|error| format!("generated FRAMES alignment is invalid: {error:?}"))?,
        group_slots: plan.group_workspace_len(),
        workspace: plan.workspace_requirements(),
        backup: plan.backup_requirements(),
    })
}

fn runtime_path(surface: SurfaceDescriptor) -> &'static str {
    match surface.sample_layout() {
        mirx::image::SampleLayout::RGB565
        | mirx::image::SampleLayout::RGB565_SWAPPED
        | mirx::image::SampleLayout::RGB888
        | mirx::image::SampleLayout::XRGB8888
        | mirx::image::SampleLayout::RGBA8888
        | mirx::image::SampleLayout::BGRA8888 => "mirui borrowed Texture",
        _ => "mirx decoded surface; backend conversion required",
    }
}

fn parse_tiles(value: &str) -> Result<Option<(u32, u32)>> {
    if value.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    let (width, height) = value
        .split_once(['x', 'X'])
        .ok_or("--tile must use <width>x<height> or none")?;
    let width = width.parse::<u32>().map_err(|_| "invalid tile width")?;
    let height = height.parse::<u32>().map_err(|_| "invalid tile height")?;
    if width == 0 || height == 0 {
        return Err("tile dimensions must be positive".into());
    }
    Ok(Some((width, height)))
}

fn storage_name(storage: FrameStorage) -> &'static str {
    match storage {
        FrameStorage::Omitted => "omitted",
        FrameStorage::Keyframe => "keyframe",
        FrameStorage::Sparse => "sparse",
        FrameStorage::Delta => "delta",
    }
}

fn encoding_name(encoding: FrameEncoding) -> &'static str {
    match encoding {
        FrameEncoding::Raw => "raw",
        FrameEncoding::Rle => "rle",
        FrameEncoding::Pixel => "pixel",
        FrameEncoding::Lz4 => "lz4",
        FrameEncoding::FrequencyReversible => "frequency-reversible",
        FrameEncoding::FrequencyQuantized(_) => "frequency-quantized",
        FrameEncoding::Delta => "frame-delta",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }

    #[test]
    fn parses_sequence_geometry_recovery_and_loss_options() {
        let options = Options::parse(&args(&[
            "--in",
            "000.png",
            "--in",
            "001.png",
            "--out",
            "animation.mirx",
            "--format",
            "rgb888",
            "--timebase",
            "90000",
            "--duration",
            "3000",
            "--play-count",
            "3",
            "--max-delta-frames",
            "12",
            "--tile",
            "16x8",
            "--input-align",
            "64",
            "--output-align",
            "64",
            "--plane-align",
            "64",
            "--width-multiple",
            "8",
            "--height-multiple",
            "2",
            "--stride-multiple",
            "64",
            "--quality",
            "70",
        ]))
        .unwrap();
        assert_eq!(options.inputs.len(), 2);
        assert_eq!(options.tiles, Some((16, 8)));
        assert_eq!(options.input_alignment, 64);
        assert_eq!(options.play_count, 3);
        assert_eq!(options.output_requirements.base_alignment(), 64);
        assert_eq!(options.output_requirements.plane_alignment(), 64);
        assert_eq!(options.output_requirements.width_multiple(), 8);
        assert_eq!(options.output_requirements.height_multiple(), 2);
        assert_eq!(options.output_requirements.stride_multiple(), 64);
        assert_eq!(options.quality, Some(70));
        assert_eq!(options.max_delta_frames, 12);
    }

    #[test]
    fn rejects_missing_frames_invalid_tiles_and_implicit_loss() {
        assert!(Options::parse(&args(&["--out", "a.mirx"])).is_err());
        assert!(
            Options::parse(&args(&["--in", "a.png", "--out", "a.mirx", "--tile", "16"])).is_err()
        );
        assert!(
            Options::parse(&args(&[
                "--in",
                "a.png",
                "--out",
                "a.mirx",
                "--quality",
                "0"
            ]))
            .is_err()
        );
        assert!(
            Options::parse(&args(&[
                "--in",
                "a.png",
                "--out",
                "a.mirx",
                "--output-align",
                "3",
            ]))
            .is_err()
        );
        assert!(
            Options::parse(&args(&[
                "--in",
                "a.png",
                "--out",
                "a.mirx",
                "--stride-multiple",
                "0",
            ]))
            .is_err()
        );
        assert_eq!(
            Options::parse(&args(&[
                "--in", "a.png", "--out", "a.mirx", "--tile", "none"
            ]))
            .unwrap()
            .tiles,
            None
        );
    }

    #[test]
    fn output_report_requires_primary_and_applies_runtime_layout() {
        use mirx::image::{ColorDescription, SampleLayout};

        let surface =
            SurfaceDescriptor::new(2, 1, SampleLayout::RGB888, ColorDescription::SRGB).unwrap();
        let sequence = FrameSequence::new(1, 1_000, 40).unwrap();
        let mut encoder = FramesEncoder::new(sequence, surface)
            .unwrap()
            .with_profiles(FrameEncodingSet::lossless().with_delta(false))
            .unwrap();
        encoder.push(&[1, 2, 3, 4, 5, 6]).unwrap();
        let encoded = encoder.finish().unwrap();

        let mut missing_primary = Document::new();
        missing_primary.push_frames(encoded.clone()).unwrap();
        let bytes = missing_primary.encode(&EncodeOptions::new()).unwrap();
        assert!(inspect_output(&bytes, SurfaceRequirements::new()).is_err());

        let mut document = Document::new();
        let id = document.push_frames(encoded).unwrap();
        document.set_primary(id).unwrap();
        let bytes = document.encode(&EncodeOptions::new()).unwrap();
        let report = inspect_output(
            &bytes,
            SurfaceRequirements::new()
                .with_base_alignment(64)
                .with_width_multiple(64)
                .with_stride_multiple(64),
        )
        .unwrap();
        assert_eq!(report.memory.byte_len(), 192);
        assert_eq!(report.memory.base_alignment(), 64);
        assert_eq!(report.memory.plane(0).unwrap().stride(), 192);
        assert_eq!(report.group_slots, 1);
        assert_eq!(report.workspace.byte_len(), 6);
        assert_eq!(report.backup.byte_len(), 0);
    }
}
