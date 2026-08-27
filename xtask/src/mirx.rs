use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs::{self, File};
use std::io::{self, Write as IoWrite};
use std::path::{Path, PathBuf};

use mirx::{
    ChunkFlags, ChunkType, CriticalAssumption, Document, Layout, OpenOptions, PayloadInput,
    PayloadLimits, PayloadLocation, PayloadValidationError, RawChunkInput, RawChunkPolicy,
    RawTypePolicy, ReadOptions, Reader, RelocationAssumption, ReservedBitsPolicy,
    TrailingBytesPolicy, crc32,
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn run(args: &[String]) -> Result {
    match args.first().map(String::as_str) {
        Some("inspect") => inspect_command(&args[1..]),
        Some("validate") => validate_command(&args[1..]),
        Some("extract") => extract_command(&args[1..]),
        Some("insert") => insert_command(&args[1..]),
        Some("replace") => replace_command(&args[1..]),
        _ => Err(usage().into()),
    }
}

fn usage() -> &'static str {
    "usage:\n  cargo xtask mirx inspect <file>\n  cargo xtask mirx validate <file> [--known-payloads]\n  cargo xtask mirx extract <file> --index <n> --out <payload> [--expect-type <u16>] [--expect-crc <u32>]\n  cargo xtask mirx insert <file> --type <u16> --payload <path> [--flags <u16>] [raw policy options]\n  cargo xtask mirx replace <file> --index <n> --payload <path> [--expect-type <u16>] [--expect-crc <u32>] [raw policy options]\nraw policy options:\n  --assume-relocatable --assume-critical-understood\n  --preserve-reserved-flags | --normalize-reserved-flags"
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

fn extract_command(args: &[String]) -> Result {
    let file = args.first().ok_or_else(usage)?;
    let mut index = None;
    let mut out = None;
    let mut expected_type = None;
    let mut expected_crc = None;
    let mut cursor = 1;
    while cursor < args.len() {
        let option = args[cursor].as_str();
        let value = args
            .get(cursor + 1)
            .ok_or_else(|| format!("{option} needs a value"))?;
        match option {
            "--index" => index = Some(parse_usize(value, "index")?),
            "--out" => out = Some(value.as_str()),
            "--expect-type" => expected_type = Some(parse_chunk_type(value)?),
            "--expect-crc" => expected_crc = Some(parse_u32(value, "CRC")?),
            _ => return Err(format!("unexpected argument: {option}").into()),
        }
        cursor += 2;
    }
    let index = index.ok_or("missing --index")?;
    let out = out.ok_or("missing --out")?;
    let input_path = Path::new(file);
    let bytes = fs::read(input_path)?;
    let payload = extract_payload(&bytes, index, expected_type, expected_crc).map_err(|error| {
        format!(
            "cannot extract chunk {index} from `{}`: {error}",
            input_path.display()
        )
    })?;
    fs::write(out, payload)?;
    println!(
        "extracted {} bytes from chunk {} to {}",
        payload.len(),
        index,
        Path::new(out).display()
    );
    Ok(())
}

fn extract_payload(
    bytes: &[u8],
    index: usize,
    expected_type: Option<ChunkType>,
    expected_crc: Option<u32>,
) -> std::result::Result<&[u8], String> {
    let options = ReadOptions::new()
        .with_payload_limits(PayloadLimits::HOST)
        .with_trailing_bytes(TrailingBytesPolicy::Preserve);
    let reader = Reader::open_with(bytes, &options)
        .map_err(|error| format!("container error: {error:?}"))?;
    let chunk = reader
        .chunks()
        .nth(index)
        .ok_or_else(|| format!("index out of bounds: {index} >= {}", reader.chunks().len()))?;
    match expected_type {
        Some(expected) if chunk.chunk_type() != expected => {
            return Err(format!(
                "type mismatch: expected {}, found {}",
                type_name(expected),
                type_name(chunk.chunk_type())
            ));
        }
        _ => {}
    }
    let actual_crc = crc32(chunk.payload());
    match expected_crc {
        Some(expected) if actual_crc != expected => {
            return Err(format!(
                "CRC mismatch: expected 0x{expected:08x}, found 0x{actual_crc:08x}"
            ));
        }
        _ => {}
    }
    Ok(chunk.payload())
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RawPolicyArgs {
    assume_relocatable: bool,
    assume_critical_understood: bool,
    reserved_flag_bits: ReservedBitsPolicy,
}

impl RawPolicyArgs {
    fn apply_option(&mut self, option: &str) -> Result<bool> {
        match option {
            "--assume-relocatable" => self.assume_relocatable = true,
            "--assume-critical-understood" => self.assume_critical_understood = true,
            "--preserve-reserved-flags" => {
                if self.reserved_flag_bits == ReservedBitsPolicy::Normalize {
                    return Err("reserved flag policies are mutually exclusive".into());
                }
                self.reserved_flag_bits = ReservedBitsPolicy::Preserve;
            }
            "--normalize-reserved-flags" => {
                if self.reserved_flag_bits == ReservedBitsPolicy::Preserve {
                    return Err("reserved flag policies are mutually exclusive".into());
                }
                self.reserved_flag_bits = ReservedBitsPolicy::Normalize;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    const fn into_policy(self) -> RawChunkPolicy {
        RawChunkPolicy {
            relocation: if self.assume_relocatable {
                RelocationAssumption::AssumeRelocatable
            } else {
                RelocationAssumption::Infer
            },
            critical_semantics: if self.assume_critical_understood {
                CriticalAssumption::AssumeCriticalUnderstood
            } else {
                CriticalAssumption::Infer
            },
            reserved_flag_bits: self.reserved_flag_bits,
        }
    }
}

fn insert_command(args: &[String]) -> Result {
    let file = args.first().ok_or_else(usage)?;
    let mut chunk_type = None;
    let mut flags = ChunkFlags::NONE;
    let mut payload_path = None;
    let mut policy = RawPolicyArgs::default();
    let mut cursor = 1;
    while cursor < args.len() {
        let option = args[cursor].as_str();
        if policy.apply_option(option)? {
            cursor += 1;
            continue;
        }
        let value = args
            .get(cursor + 1)
            .ok_or_else(|| format!("{option} needs a value"))?;
        match option {
            "--type" => chunk_type = Some(parse_chunk_type(value)?),
            "--flags" => flags = ChunkFlags::from_bits_retain(parse_u16(value, "flags")?),
            "--payload" => payload_path = Some(value.as_str()),
            _ => return Err(format!("unexpected argument: {option}").into()),
        }
        cursor += 2;
    }
    let chunk_type = chunk_type.ok_or("missing --type")?;
    let payload_path = payload_path.ok_or("missing --payload")?;
    let source_path = Path::new(file);
    let source = fs::read(source_path)?;
    let payload = fs::read(payload_path)?;
    let payload_len = payload.len();
    let output = insert_raw_bytes(source, chunk_type, flags, payload, policy.into_policy())
        .map_err(|error| format!("cannot insert into `{}`: {error}", source_path.display()))?;
    replace_file_atomically(source_path, &output)?;
    println!(
        "inserted type={} flags=0x{:04x} size={} into {}",
        type_name(chunk_type),
        flags.bits(),
        payload_len,
        source_path.display()
    );
    Ok(())
}

fn replace_command(args: &[String]) -> Result {
    let file = args.first().ok_or_else(usage)?;
    let mut index = None;
    let mut payload_path = None;
    let mut expected_type = None;
    let mut expected_crc = None;
    let mut policy = RawPolicyArgs::default();
    let mut cursor = 1;
    while cursor < args.len() {
        let option = args[cursor].as_str();
        if policy.apply_option(option)? {
            cursor += 1;
            continue;
        }
        let value = args
            .get(cursor + 1)
            .ok_or_else(|| format!("{option} needs a value"))?;
        match option {
            "--index" => index = Some(parse_usize(value, "index")?),
            "--payload" => payload_path = Some(value.as_str()),
            "--expect-type" => expected_type = Some(parse_chunk_type(value)?),
            "--expect-crc" => expected_crc = Some(parse_u32(value, "CRC")?),
            _ => return Err(format!("unexpected argument: {option}").into()),
        }
        cursor += 2;
    }
    let index = index.ok_or("missing --index")?;
    let payload_path = payload_path.ok_or("missing --payload")?;
    let source_path = Path::new(file);
    let source = fs::read(source_path)?;
    let payload = fs::read(payload_path)?;
    let payload_len = payload.len();
    let output = replace_raw_bytes(
        source,
        index,
        expected_type,
        expected_crc,
        payload,
        policy.into_policy(),
    )
    .map_err(|error| {
        format!(
            "cannot replace chunk {index} in `{}`: {error}",
            source_path.display()
        )
    })?;
    replace_file_atomically(source_path, &output)?;
    println!(
        "replaced chunk {} with {} bytes in {}",
        index,
        payload_len,
        source_path.display()
    );
    Ok(())
}

fn insert_raw_bytes(
    source: Vec<u8>,
    chunk_type: ChunkType,
    flags: ChunkFlags,
    payload: Vec<u8>,
    policy: RawChunkPolicy,
) -> std::result::Result<Vec<u8>, String> {
    let raw_policies = [RawTypePolicy { chunk_type, policy }];
    let open_options = OpenOptions::host_tools().with_raw_type_policies(&raw_policies);
    let mut document = Document::from_vec_with(source, &open_options)
        .map_err(|error| format!("container error: {error:?}"))?;
    document
        .push_raw(RawChunkInput {
            chunk_type,
            flags,
            payload: PayloadInput::Owned(payload),
            policy,
        })
        .map_err(|error| format!("edit error: {error:?}"))?;
    document
        .finish()
        .map(|bytes| bytes.into_owned())
        .map_err(|error| format!("encode error: {error:?}"))
}

fn replace_raw_bytes(
    source: Vec<u8>,
    index: usize,
    expected_type: Option<ChunkType>,
    expected_crc: Option<u32>,
    payload: Vec<u8>,
    policy: RawChunkPolicy,
) -> std::result::Result<Vec<u8>, String> {
    if matches!(
        policy.critical_semantics,
        CriticalAssumption::AssumeCriticalUnderstood
    ) && expected_type.is_none()
    {
        return Err("--assume-critical-understood requires --expect-type".into());
    }
    let raw_policy = expected_type.map(|chunk_type| RawTypePolicy { chunk_type, policy });
    let raw_policies = raw_policy.as_slice();
    let open_options = OpenOptions::host_tools().with_raw_type_policies(raw_policies);
    let mut document = Document::from_vec_with(source, &open_options)
        .map_err(|error| format!("container error: {error:?}"))?;
    let id = guarded_document_chunk(&document, index, expected_type, expected_crc)?;
    document
        .replace_raw(id, PayloadInput::Owned(payload), policy)
        .map_err(|error| format!("edit error: {error:?}"))?;
    document
        .finish()
        .map(|bytes| bytes.into_owned())
        .map_err(|error| format!("encode error: {error:?}"))
}

fn guarded_document_chunk(
    document: &Document<'_>,
    index: usize,
    expected_type: Option<ChunkType>,
    expected_crc: Option<u32>,
) -> std::result::Result<mirx::ChunkId, String> {
    let chunk = document.chunks().nth(index).ok_or_else(|| {
        format!(
            "index out of bounds: {index} >= {}",
            document.chunks().len()
        )
    })?;
    match expected_type {
        Some(expected) if chunk.chunk_type() != expected => {
            return Err(format!(
                "type mismatch: expected {}, found {}",
                type_name(expected),
                type_name(chunk.chunk_type())
            ));
        }
        _ => {}
    }
    let payload = chunk
        .payload_bytes()
        .ok_or_else(|| "chunk payload is not contiguous".to_owned())?;
    match expected_crc {
        Some(expected) if crc32(payload) != expected => {
            return Err(format!(
                "CRC mismatch: expected 0x{expected:08x}, found 0x{:08x}",
                crc32(payload)
            ));
        }
        _ => {}
    }
    Ok(chunk.id())
}

fn replace_file_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let metadata = fs::metadata(path)?;
    let (temporary_path, mut temporary) = create_sibling_temporary(path)?;
    let prepared = (|| {
        IoWrite::write_all(&mut temporary, bytes)?;
        temporary.sync_all()?;
        fs::set_permissions(&temporary_path, metadata.permissions())?;
        Ok::<_, io::Error>(())
    })();
    drop(temporary);
    if let Err(error) = prepared {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    Ok(())
}

fn create_sibling_temporary(path: &Path) -> io::Result<(PathBuf, File)> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?
        .to_string_lossy();
    for attempt in 0..100u8 {
        let candidate = parent.join(format!(
            ".{file_name}.mirx-tmp-{}-{attempt}",
            std::process::id()
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "cannot create a unique sibling temporary file",
    ))
}

fn parse_chunk_type(value: &str) -> Result<ChunkType> {
    let raw = parse_u16(value, "chunk type")?;
    ChunkType::new(raw).ok_or_else(|| "chunk type zero is reserved".into())
}

fn parse_usize(value: &str, name: &str) -> Result<usize> {
    value
        .parse::<usize>()
        .map_err(|_| format!("invalid {name}: `{value}`").into())
}

fn parse_u16(value: &str, name: &str) -> Result<u16> {
    parse_radix(value, name).and_then(|parsed| {
        u16::try_from(parsed).map_err(|_| format!("{name} out of range: `{value}`").into())
    })
}

fn parse_u32(value: &str, name: &str) -> Result<u32> {
    parse_radix(value, name).and_then(|parsed| {
        u32::try_from(parsed).map_err(|_| format!("{name} out of range: `{value}`").into())
    })
}

fn parse_radix(value: &str, name: &str) -> Result<u64> {
    let parsed = if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16)
    } else {
        value.parse::<u64>()
    };
    parsed.map_err(|_| format!("invalid {name}: `{value}`").into())
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
    use std::sync::atomic::{AtomicU32, Ordering};

    static TEMP_FILE_COUNTER: AtomicU32 = AtomicU32::new(0);

    const fn relocatable_policy() -> RawChunkPolicy {
        RawChunkPolicy {
            relocation: RelocationAssumption::AssumeRelocatable,
            critical_semantics: CriticalAssumption::Infer,
            reserved_flag_bits: ReservedBitsPolicy::Reject,
        }
    }

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

    #[test]
    fn extraction_returns_exact_payload_and_checks_type_and_crc_guards() {
        let custom = ChunkType::new(0xbeef).unwrap();
        let bytes = encode_chunks(&[
            (ChunkType::META.raw(), 0, b"first"),
            (custom.raw(), 0, b"second"),
        ]);
        let expected_crc = crc32(b"second");

        assert_eq!(
            extract_payload(&bytes, 1, Some(custom), Some(expected_crc)),
            Ok(b"second".as_slice())
        );
        assert!(
            extract_payload(&bytes, 1, Some(ChunkType::META), None)
                .unwrap_err()
                .contains("type mismatch")
        );
        assert!(
            extract_payload(&bytes, 1, None, Some(expected_crc ^ 1))
                .unwrap_err()
                .contains("CRC mismatch")
        );
        assert!(
            extract_payload(&bytes, 2, None, None)
                .unwrap_err()
                .contains("index out of bounds")
        );
    }

    #[test]
    fn numeric_guards_accept_decimal_and_prefixed_hex() {
        assert_eq!(parse_chunk_type("16").unwrap(), ChunkType::META);
        assert_eq!(parse_chunk_type("0x0010").unwrap(), ChunkType::META);
        assert_eq!(parse_u32("0xdeadbeef", "CRC").unwrap(), 0xdead_beef);
        assert!(parse_chunk_type("0").is_err());
        assert!(parse_u16("65536", "chunk type").is_err());
        assert!(parse_usize("not-a-number", "index").is_err());
    }

    #[test]
    fn raw_insert_and_guarded_replace_rewrite_exact_selected_payloads() {
        let custom = ChunkType::new(0xbeef).unwrap();
        let inserted = insert_raw_bytes(
            encode_chunks(&[]),
            custom,
            ChunkFlags::NONE,
            b"first".to_vec(),
            relocatable_policy(),
        )
        .unwrap();
        let inserted_reader = Reader::open(&inserted).unwrap();
        let inserted_chunk = inserted_reader.chunks().next().unwrap();
        assert_eq!(inserted_chunk.chunk_type(), custom);
        assert_eq!(inserted_chunk.payload(), b"first");

        let mut source = encode_chunks(&[(custom.raw(), 0, b"old")]);
        clear_primary(&mut source);
        let old_crc = crc32(b"old");
        let replaced = replace_raw_bytes(
            source.clone(),
            0,
            Some(custom),
            Some(old_crc),
            b"new".to_vec(),
            relocatable_policy(),
        )
        .unwrap();
        assert_eq!(
            Reader::open(&replaced)
                .unwrap()
                .chunks()
                .next()
                .unwrap()
                .payload(),
            b"new"
        );
        assert!(
            replace_raw_bytes(
                source,
                0,
                Some(ChunkType::META),
                Some(old_crc),
                b"ignored".to_vec(),
                relocatable_policy(),
            )
            .unwrap_err()
            .contains("type mismatch")
        );

        let mut critical_source =
            encode_chunks(&[(custom.raw(), ChunkFlags::CRITICAL.bits(), b"critical")]);
        clear_primary(&mut critical_source);
        let critical_policy = RawChunkPolicy {
            relocation: RelocationAssumption::AssumeRelocatable,
            critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
            reserved_flag_bits: ReservedBitsPolicy::Reject,
        };
        assert!(
            replace_raw_bytes(
                critical_source.clone(),
                0,
                None,
                None,
                b"new".to_vec(),
                critical_policy,
            )
            .unwrap_err()
            .contains("requires --expect-type")
        );
        let replaced_critical = replace_raw_bytes(
            critical_source,
            0,
            Some(custom),
            Some(crc32(b"critical")),
            b"new-critical".to_vec(),
            critical_policy,
        )
        .unwrap();
        let raw_policy = [RawTypePolicy {
            chunk_type: custom,
            policy: critical_policy,
        }];
        let reopened = Document::open_with(
            &replaced_critical,
            &OpenOptions::host_tools().with_raw_type_policies(&raw_policy),
        )
        .unwrap();
        assert_eq!(
            reopened.chunks().next().unwrap().payload_bytes().unwrap(),
            b"new-critical"
        );
    }

    #[test]
    fn raw_policy_options_are_explicit_and_mutually_exclusive() {
        let mut args = RawPolicyArgs::default();
        assert!(args.apply_option("--assume-relocatable").unwrap());
        assert!(args.apply_option("--assume-critical-understood").unwrap());
        assert!(args.apply_option("--preserve-reserved-flags").unwrap());
        assert_eq!(
            args.into_policy(),
            RawChunkPolicy {
                relocation: RelocationAssumption::AssumeRelocatable,
                critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
                reserved_flag_bits: ReservedBitsPolicy::Preserve,
            }
        );

        let mut conflicting = RawPolicyArgs::default();
        conflicting
            .apply_option("--normalize-reserved-flags")
            .unwrap();
        assert!(
            conflicting
                .apply_option("--preserve-reserved-flags")
                .is_err()
        );
        assert!(!conflicting.apply_option("--payload").unwrap());
    }

    #[test]
    fn file_replacement_uses_a_flushed_sibling_and_preserves_permissions() {
        let sequence = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mirx-atomic-replace-{}-{sequence}.bin",
            std::process::id()
        ));
        fs::write(&path, b"before").unwrap();
        let permissions = fs::metadata(&path).unwrap().permissions();

        replace_file_atomically(&path, b"after").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"after");
        assert_eq!(fs::metadata(&path).unwrap().permissions(), permissions);
        fs::remove_file(path).unwrap();
    }
}
