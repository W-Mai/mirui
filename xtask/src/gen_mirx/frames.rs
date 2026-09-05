use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use mirx::{
    ChunkFlags, ChunkType, Document, EncodeOptions, FrameEncoding, FrameEncodingSet, FramePolicy,
    FrameSequence, FrameStorage, FramesEncoder, PayloadLimits, Reader, image::SurfaceDescriptor,
};

use super::{Result, icu_program, probe_icu};

#[derive(Debug, Eq, PartialEq)]
struct Options {
    inputs: Vec<PathBuf>,
    output: PathBuf,
    format: String,
    timebase: u32,
    duration: u32,
    max_delta_frames: u16,
    tiles: Option<(u32, u32)>,
    input_alignment: u32,
    quality: Option<u8>,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        let mut inputs = Vec::new();
        let mut output = None;
        let mut format = String::from("rgba8888");
        let mut timebase = 1_000;
        let mut duration = 40;
        let mut max_delta_frames = 8;
        let mut tiles = Some((32, 32));
        let mut input_alignment = 1u32;
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
        if quality.is_some_and(|value| !(1..=100).contains(&value)) {
            return Err("--quality must be between 1 and 100".into());
        }
        Ok(Self {
            inputs,
            output,
            format,
            timebase,
            duration,
            max_delta_frames,
            tiles,
            input_alignment,
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

pub fn run(args: &[String]) -> Result {
    probe_icu()?;
    let options = Options::parse(args)?;
    let mut frames = Vec::with_capacity(options.inputs.len());
    for input in &options.inputs {
        frames.push(decode_frame(input, &options.format)?);
    }
    let first = frames.first().expect("validated frame list");
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
    document
        .push_frames_with_flags(encoded, ChunkFlags::CRITICAL)
        .map_err(|error| format!("cannot add encoded frames: {error:?}"))?;
    let bytes = document
        .encode(&EncodeOptions::new())
        .map_err(|error| format!("cannot encode FRAMES container: {error:?}"))?;
    verify_output(&bytes)?;
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

fn verify_output(bytes: &[u8]) -> Result {
    let reader = Reader::open(bytes)
        .map_err(|error| format!("generated FRAMES container is invalid: {error:?}"))?;
    reader
        .validate_known_payloads(&PayloadLimits::HOST)
        .map_err(|error| format!("generated FRAMES preflight failed: {error:?}"))?;
    let entry = reader
        .chunks()
        .find(|entry| entry.chunk_type() == ChunkType::FRAMES)
        .ok_or("generated container has no FRAMES chunk")?;
    entry
        .frames(&PayloadLimits::HOST)
        .map_err(|error| format!("generated FRAMES payload is invalid: {error:?}"))?
        .ok_or("generated chunk type is not FRAMES")?
        .validate_data()
        .map_err(|error| format!("generated FRAMES DATA is invalid: {error:?}"))?;
    Ok(())
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
            "--max-delta-frames",
            "12",
            "--tile",
            "16x8",
            "--input-align",
            "64",
            "--quality",
            "70",
        ]))
        .unwrap();
        assert_eq!(options.inputs.len(), 2);
        assert_eq!(options.tiles, Some((16, 8)));
        assert_eq!(options.input_alignment, 64);
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
        assert_eq!(
            Options::parse(&args(&[
                "--in", "a.png", "--out", "a.mirx", "--tile", "none"
            ]))
            .unwrap()
            .tiles,
            None
        );
    }
}
