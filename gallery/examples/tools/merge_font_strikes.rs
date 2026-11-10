use mirx::document::EncodeOptions;
use mirx::font::{Font, FontAsset, RepresentationAsset};
use mirx::reader::PayloadLimits;
use mirx::{ChunkType, Document, Reader};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        return Err(
            "usage: merge_font_strikes input1.mirx input2.mirx [inputs...] output.mirx".into(),
        );
    }
    let out = args.last().ok_or("input fonts followed by output")?;
    let mut fonts = Vec::new();
    for path in &args[..args.len() - 1] {
        let bytes = std::fs::read(path)?;
        let reader = Reader::open(&bytes).map_err(|e| format!("reader {e:?}"))?;
        let mut chunks = reader
            .chunks()
            .filter(|c| c.chunk_type() == ChunkType::FONT);
        let chunk = chunks.next().ok_or("FONT missing")?;
        if chunks.next().is_some() {
            return Err("exactly one FONT chunk per input is required".into());
        }
        fonts.push(Font::decode(chunk.payload()).map_err(|e| format!("decode {e:?}"))?);
    }
    let first = fonts.first().ok_or("fonts missing")?;
    let mut reps = Vec::new();
    let mut maps = Vec::new();
    let mut surfaces = Vec::new();
    let mut metrics = Vec::new();
    for (index, font) in fonts.iter().enumerate() {
        if font.face() != first.face()
            || font.advance_source() != first.advance_source()
            || font.cmap() != first.cmap()
            || font.glyph_ids() != first.glyph_ids()
        {
            return Err("font faces, advances or glyph identities differ".into());
        }
        if font.representation_count() != 1 || font.surface_count() != 1 {
            return Err("one strike per input".into());
        }
        let surface = font.surface(0).unwrap();
        maps.push(surface.map().atlas_map().ok_or("atlas required")?);
        reps.push(
            RepresentationAsset::new(font.representation(0).unwrap().metadata(), index as u16)
                .with_atlas_map(index as u32),
        );
        surfaces.push(surface);
        metrics.extend_from_slice(font.raster_metrics());
    }
    let mut asset = FontAsset::new(first.face(), first.cmap(), first.advance_source())
        .with_rasters(&reps, &metrics, &surfaces)
        .with_atlas_maps(&maps);
    if let Some(ids) = first.glyph_ids() {
        asset = asset.with_glyph_ids(ids)
    }
    let font = Font::from_asset(asset, &PayloadLimits::HOST).map_err(|e| format!("asset {e:?}"))?;
    let mut document = Document::new();
    document
        .push_font(&font)
        .map_err(|e| format!("document {e:?}"))?;
    let bytes = document
        .encode(&EncodeOptions::new())
        .map_err(|e| format!("encode {e:?}"))?;
    std::fs::write(out, &bytes)?;
    println!("{} strikes, {} bytes -> {out}", fonts.len(), bytes.len());
    Ok(())
}
