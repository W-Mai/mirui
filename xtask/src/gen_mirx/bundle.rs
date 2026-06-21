//! `gen-mirx bundle` — merge several single-font `.mirx` files into one
//! multi-representation bundle.
//!
//! Each input already holds one prefixed FONT chunk (built by
//! `gen-mirx font`). This pulls each input's FONT payload out verbatim
//! and re-packs them into a single CHUNK file, so `MultiFontProvider`
//! can pick a representation per size at runtime. No re-rasterization —
//! purely a container merge.

use std::fs;
use std::path::PathBuf;

use mirx::{ChunkEntry, chunk_type, encode_chunks, parse_chunk};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn run(args: &[String]) -> Result {
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

    // Read every input fully first; encode_chunks borrows the payload
    // slices, so the buffers must outlive the call.
    let buffers: Vec<Vec<u8>> = inputs
        .iter()
        .map(fs::read)
        .collect::<std::io::Result<_>>()?;

    let mut payloads: Vec<&[u8]> = Vec::with_capacity(buffers.len());
    for (path, buf) in inputs.iter().zip(&buffers) {
        let parsed = parse_chunk(buf).map_err(|e| format!("{}: {e:?}", path.display()))?;
        let payload = parsed
            .chunk_payload(buf, chunk_type::FONT)
            .ok_or_else(|| format!("{}: no FONT chunk", path.display()))?;
        payloads.push(payload);
    }

    let chunks: Vec<(u16, u16, &[u8])> = payloads
        .iter()
        .map(|p| (chunk_type::FONT, ChunkEntry::FLAG_CRITICAL, *p))
        .collect();
    let bytes = encode_chunks(&chunks);
    fs::write(&out, &bytes)?;

    println!(
        "wrote {} bytes to {} ({} font chunks merged)",
        bytes.len(),
        out.display(),
        payloads.len(),
    );
    Ok(())
}
