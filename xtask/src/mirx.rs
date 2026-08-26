use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;
use std::path::Path;

use mirx::{
    ChunkType, Layout, PayloadLimits, PayloadLocation, PayloadValidationError, ReadOptions, Reader,
    TrailingBytesPolicy, crc32,
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn run(args: &[String]) -> Result {
    match args.first().map(String::as_str) {
        Some("inspect") => inspect_command(&args[1..]),
        Some("validate") => validate_command(&args[1..]),
        _ => Err(usage().into()),
    }
}

fn usage() -> &'static str {
    "usage:\n  cargo xtask mirx inspect <file>\n  cargo xtask mirx validate <file> [--known-payloads]"
}

fn inspect_command(args: &[String]) -> Result {
    let [file] = args else {
        return Err(usage().into());
    };
    let path = Path::new(file);
    let bytes = fs::read(path)?;
    let report = inspect_bytes(&bytes).map_err(|error| {
        format!(
            "cannot inspect `{}` as a MIRX document: {error}",
            path.display()
        )
    })?;
    print!("{report}");
    Ok(())
}

fn validate_command(args: &[String]) -> Result {
    let (file, known_payloads) = match args {
        [file] => (file, false),
        [file, option] if option == "--known-payloads" => (file, true),
        _ => return Err(usage().into()),
    };
    let path = Path::new(file);
    let bytes = fs::read(path)?;
    let status = validate_bytes(&bytes, known_payloads)
        .map_err(|error| format!("validation failed for `{}`: {error}", path.display()))?;
    print!("{status}");
    Ok(())
}

fn validate_bytes(bytes: &[u8], known_payloads: bool) -> std::result::Result<String, String> {
    let options = ReadOptions::new().with_payload_limits(PayloadLimits::HOST);
    let reader = Reader::open_with(bytes, &options)
        .map_err(|error| format!("container error: {error:?}"))?;
    let mut report = String::new();
    for finding in reader.compliance_findings() {
        writeln!(report, "warning: {finding:?}").unwrap();
    }
    if known_payloads {
        reader
            .validate_known_payloads(&PayloadLimits::HOST)
            .map_err(format_payload_validation_error)?;
        writeln!(report, "valid container and known payloads").unwrap();
    } else {
        writeln!(report, "valid container").unwrap();
    }
    Ok(report)
}

fn format_payload_validation_error(error: PayloadValidationError) -> String {
    match error.location() {
        PayloadLocation::FlatImage => {
            format!("known payload error at flat image: {:?}", error.failure())
        }
        PayloadLocation::Chunk {
            index,
            chunk_type,
            payload_offset,
        } => format!(
            "known payload error at chunk index={} type={} offset={}: {:?}",
            index,
            type_name(chunk_type),
            payload_offset,
            error.failure(),
        ),
        _ => format!(
            "known payload error at unknown location: {:?}",
            error.failure()
        ),
    }
}

fn inspect_bytes(bytes: &[u8]) -> std::result::Result<String, String> {
    let options = ReadOptions::new()
        .with_payload_limits(PayloadLimits::HOST)
        .with_trailing_bytes(TrailingBytesPolicy::Preserve);
    let reader = Reader::open_with(bytes, &options).map_err(|error| format!("{error:?}"))?;
    let header = reader.file_header();
    let mut report = String::new();
    writeln!(
        report,
        "version={}.{} layout={} file_flags=0x{:02x} logical_size={} trailing_size={} future={}",
        header.version_major,
        header.version_minor,
        layout_name(reader.layout()),
        header.flags,
        reader.logical_len(),
        reader.trailing_bytes().len(),
        reader.has_future_semantics(),
    )
    .unwrap();

    let hints = reader.primary_hints();
    match reader.layout() {
        Layout::Flat => writeln!(
            report,
            "primary=flat color_format=0x{:02x} width={} height={} stride={}",
            hints.color_format_raw(),
            hints.width(),
            hints.height(),
            hints.stride(),
        )
        .unwrap(),
        Layout::Chunk => match reader.primary().map_err(|error| format!("{error:?}"))? {
            Some(primary) => writeln!(
                report,
                "primary=index:{} type:{} color_format=0x{:02x} width={} height={} stride={}",
                primary.index(),
                type_name(primary.chunk_type()),
                hints.color_format_raw(),
                hints.width(),
                hints.height(),
                hints.stride(),
            )
            .unwrap(),
            None => writeln!(
                report,
                "primary=none color_format=0x{:02x} width={} height={} stride={}",
                hints.color_format_raw(),
                hints.width(),
                hints.height(),
                hints.stride(),
            )
            .unwrap(),
        },
    }

    let chunks = reader.chunks();
    writeln!(report, "chunks={}", chunks.len()).unwrap();
    let mut occurrences = BTreeMap::<u16, usize>::new();
    for chunk in chunks {
        let raw_type = chunk.chunk_type().raw();
        let occurrence = occurrences.entry(raw_type).or_default();
        writeln!(
            report,
            "index={} type={} occurrence={} flags=0x{:04x} size={} crc32=0x{:08x}",
            chunk.index(),
            type_name(chunk.chunk_type()),
            *occurrence,
            chunk.flags().bits(),
            chunk.payload().len(),
            crc32(chunk.payload()),
        )
        .unwrap();
        *occurrence += 1;
    }
    Ok(report)
}

const fn layout_name(layout: Layout) -> &'static str {
    match layout {
        Layout::Flat => "flat",
        Layout::Chunk => "chunk",
    }
}

fn type_name(chunk_type: ChunkType) -> String {
    let label = match chunk_type {
        ChunkType::IMAGE => "IMAGE",
        ChunkType::FRAMES => "FRAMES",
        ChunkType::VECTOR => "VECTOR",
        ChunkType::FONT => "FONT",
        ChunkType::META => "META",
        ChunkType::PALETTE => "PALETTE",
        _ => "CUSTOM",
    };
    format!("0x{:04x}({label})", chunk_type.raw())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mirx::{ColorFormat, FlatImageInput, PrimaryHints, encode_chunks, encode_flat};

    fn set_primary(bytes: &mut [u8], chunk_type: ChunkType, hints: PrimaryHints) {
        bytes[20..22].copy_from_slice(&chunk_type.raw().to_le_bytes());
        bytes[22] = hints.color_format_raw();
        bytes[24..28].copy_from_slice(&hints.width().to_le_bytes());
        bytes[28..32].copy_from_slice(&hints.height().to_le_bytes());
        bytes[32..36].copy_from_slice(&hints.stride().to_le_bytes());
        let checksum = crc32(&bytes[..40]);
        bytes[40..44].copy_from_slice(&checksum.to_le_bytes());
    }

    fn clear_primary(bytes: &mut [u8]) {
        bytes[20..36].fill(0);
        let checksum = crc32(&bytes[..40]);
        bytes[40..44].copy_from_slice(&checksum.to_le_bytes());
    }

    #[test]
    fn chunk_report_has_stable_indices_occurrences_flags_sizes_and_checksums() {
        let custom = ChunkType::new(0xbeef).unwrap();
        let mut bytes = encode_chunks(&[
            (ChunkType::META.raw(), 0, b"one"),
            (custom.raw(), 0x8000, b"custom"),
            (ChunkType::META.raw(), 2, b"two"),
        ]);
        let hints = PrimaryHints::new(mirx::PRIMARY_FORMAT_NONE, 12, 8, 0);
        set_primary(&mut bytes, ChunkType::META, hints);

        let report = inspect_bytes(&bytes).unwrap();
        assert!(report.starts_with("version=1.0 layout=chunk file_flags=0x00 logical_size="));
        assert!(report.contains(
            "primary=index:0 type:0x0010(META) color_format=0xff width=12 height=8 stride=0\n"
        ));
        assert!(report.contains("chunks=3\n"));
        assert!(report.contains(&format!(
            "index=0 type=0x0010(META) occurrence=0 flags=0x0000 size=3 crc32=0x{:08x}\n",
            crc32(b"one")
        )));
        assert!(report.contains(&format!(
            "index=1 type=0xbeef(CUSTOM) occurrence=0 flags=0x8000 size=6 crc32=0x{:08x}\n",
            crc32(b"custom")
        )));
        assert!(report.contains(&format!(
            "index=2 type=0x0010(META) occurrence=1 flags=0x0002 size=3 crc32=0x{:08x}\n",
            crc32(b"two")
        )));
    }

    #[test]
    fn flat_and_trailing_bytes_are_reported_without_chunk_rows() {
        let mut bytes = encode_flat(&FlatImageInput {
            width: 2,
            height: 1,
            stride: ColorFormat::A8.minimum_stride(2).unwrap(),
            format: ColorFormat::A8,
            main: &[1, 2],
            extra: None,
        });
        bytes.extend_from_slice(b"tail");

        let report = inspect_bytes(&bytes).unwrap();
        assert!(report.contains("layout=flat"));
        assert!(report.contains("trailing_size=4"));
        assert!(report.contains("primary=flat color_format=0x23 width=2 height=1 stride=2\n"));
        assert!(report.ends_with("chunks=0\n"));
    }

    #[test]
    fn malformed_input_and_wrong_arguments_return_contextual_errors() {
        assert!(inspect_bytes(b"not mirx").unwrap_err().contains("BadMagic"));
        assert_eq!(run(&["inspect".into()]).unwrap_err().to_string(), usage());
        assert_eq!(run(&["unknown".into()]).unwrap_err().to_string(), usage());
    }

    #[test]
    fn validation_separates_container_compliance_from_known_payloads() {
        let mut malformed_meta = encode_chunks(&[(ChunkType::META.raw(), 0, b"bad")]);
        clear_primary(&mut malformed_meta);

        assert_eq!(
            validate_bytes(&malformed_meta, false),
            Ok("valid container\n".into())
        );
        let error = validate_bytes(&malformed_meta, true).unwrap_err();
        assert!(error.contains("chunk index=0"));
        assert!(error.contains("type=0x0010(META)"));
        assert!(error.contains("Meta(UnsupportedVersion"), "{error}");

        let mut legacy_hints = encode_chunks(&[(ChunkType::META.raw(), 0, b"bad")]);
        set_primary(&mut legacy_hints, ChunkType::META, PrimaryHints::ZERO);
        let report = validate_bytes(&legacy_hints, false).unwrap();
        assert!(report.contains("warning: LegacyZeroPrimaryHints"));
        assert!(report.ends_with("valid container\n"));
    }

    #[test]
    fn validation_rejects_trailing_and_structurally_invalid_sources() {
        let valid = encode_flat(&FlatImageInput {
            width: 1,
            height: 1,
            stride: 1,
            format: ColorFormat::A8,
            main: &[7],
            extra: None,
        });
        assert_eq!(
            validate_bytes(&valid, true),
            Ok("valid container and known payloads\n".into())
        );

        let mut trailing = valid.clone();
        trailing.push(0);
        assert!(
            validate_bytes(&trailing, false)
                .unwrap_err()
                .contains("TrailingBytes")
        );
        assert!(
            validate_bytes(b"MIRX", false)
                .unwrap_err()
                .contains("Truncated")
        );
    }
}
