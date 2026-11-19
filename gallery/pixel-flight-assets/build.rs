use mirui_sprite_stack_format::{
    StackDescriptor,
    lod::{LodColor, LodError, StackLods, bake},
};
use mirx::image::SampleLayout;
use mirx::meta::MetaValueRef;
use mirx::reader::PayloadLimits;
use mirx::{ChunkType, Reader};
use std::fmt::Write;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=jet.mirx");
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    prepare(include_bytes!("jet.mirx"), &output)
        .expect("invalid Pixel Flight jet.mirx or unwritable build output");
}

// Pixel Flight indexes four lighting materials, with up to sixteen shades per material.
fn material(color: u8) -> usize {
    usize::from((color - 1) >> 4)
}

fn vertical_pivot(reader: &Reader<'_>) -> Result<i32, String> {
    let mut pivot = None;
    for chunk in reader
        .chunks()
        .filter(|c| c.chunk_type() == ChunkType::META)
    {
        let meta = chunk
            .meta(&PayloadLimits::HOST)
            .map_err(|e| format!("{e:?}"))?
            .ok_or("invalid META chunk")?;
        for entry in meta.get_all("pixel-flight.pivot-z-q8") {
            let MetaValueRef::Bytes(bytes) = entry.value else {
                return Err("vertical pivot must contain Q24.8 bytes".into());
            };
            let value = i32::from_le_bytes(
                bytes
                    .try_into()
                    .map_err(|_| "invalid vertical pivot length")?,
            );
            if pivot.replace(value).is_some() {
                return Err("duplicate vertical pivot".into());
            }
        }
    }
    pivot.ok_or_else(|| "missing pixel-flight.pivot-z-q8".into())
}

#[derive(Default)]
struct Level {
    voxels: Vec<[u8; 3]>,
    ends: Vec<usize>,
}

impl Level {
    fn sample(&mut self, x: usize, y: usize, color: u8) {
        if color != 0 {
            self.voxels.push([x as u8, y as u8, color]);
        }
    }

    fn finish_layer(&mut self) {
        self.ends.push(self.voxels.len());
    }

    fn write(&self, out: &mut String, prefix: &str) {
        writeln!(
            out,
            "pub static {prefix}VOXELS: [StackSample; {}] = [",
            self.voxels.len()
        )
        .unwrap();
        for [x, y, color] in &self.voxels {
            writeln!(out, "StackSample {{ x: {x}, y: {y}, color: {color} }},").unwrap();
        }
        writeln!(
            out,
            "];\npub const {prefix}STARTS: [usize; LAYERS + 1] = [0,"
        )
        .unwrap();
        for end in &self.ends {
            writeln!(out, "{end},").unwrap();
        }
        writeln!(out, "];").unwrap();
    }
}

fn prepare(bytes: &[u8], output: &Path) -> Result<(), String> {
    let reader = Reader::open(bytes).map_err(|e| format!("{e:?}"))?;
    let (stack, image) =
        StackDescriptor::from_reader_with_atlas(&reader).map_err(|e| e.to_string())?;
    let (width, length) = stack.canvas();
    if width > 256 || length > 256 || width % 2 != 0 || length % 2 != 0 || stack.spacing_q8() != 256
    {
        return Err(
            "aircraft LOD requires even dimensions up to 256 and unit layer spacing".into(),
        );
    }
    let (width, length) = (usize::from(width), usize::from(length));
    let raw = image.raw().ok_or("expected a RAW atlas")?;
    if raw.surface().sample_layout() != SampleLayout::I8 {
        return Err("expected an I8 aircraft material atlas".into());
    }
    let palette = image.color_table().ok_or("missing atlas palette")?;
    let mut opaque = [false; 256];
    let mut colors = Vec::new();
    for (index, rgba) in palette.as_bytes().chunks_exact(4).enumerate() {
        match rgba[3] {
            0 => continue,
            255 if (1..=64).contains(&index) => (),
            _ => {
                return Err(format!(
                    "invalid aircraft material or alpha at palette index {index}"
                ));
            }
        }
        let color = index as u8;
        opaque[index] = true;
        colors.push([color, rgba[0], rgba[1], rgba[2], material(color) as u8]);
    }

    let plane = raw.plane(0).ok_or("missing atlas plane")?;
    let mut full = Level::default();
    for region in stack.layers() {
        if region.width() != 0 {
            for y in 0..region.height() {
                let row = plane
                    .row(region.y() + y)
                    .map_err(|e| format!("{e:?}"))?
                    .ok_or("missing atlas row")?;
                for (x, &color) in row[region.x() as usize..region.right() as usize]
                    .iter()
                    .enumerate()
                {
                    if color != 0 && !opaque[usize::from(color)] {
                        return Err(format!("missing material at palette index {color}"));
                    }
                    full.sample(x, y as usize, color);
                }
            }
        }
        full.finish_layer();
    }
    let payload = bake::encode(
        stack,
        bake::Samples::Indexed(palette.as_bytes()),
        &[1, 2],
        &[[1, 16], [17, 32], [33, 48], [49, 64]],
        1024 * 1024,
        |x, y| {
            let row = plane.row(y).ok().flatten().ok_or(LodError::InvalidSample)?;
            Ok(LodColor::Indexed(row[x as usize]))
        },
    )
    .map_err(|e| e.to_string())?
    .ok_or("missing aircraft LOD")?;
    let lods = StackLods::open(&payload, stack, image).map_err(|e| e.to_string())?;
    let level = lods.level(0).ok_or("missing aircraft LOD")?;
    let mut coarse = Level::default();
    for layer in 0..stack.layers().len() {
        for cell in level.layer(layer).ok_or("missing aircraft layer")? {
            let LodColor::Indexed(index) = cell.color else {
                return Err("expected an indexed aircraft".into());
            };
            coarse.sample(usize::from(cell.x), usize::from(cell.y), index);
        }
        coarse.finish_layer();
    }

    let [x, y] = stack.pivot_q8();
    let z = vertical_pivot(&reader)?;
    let mut source = String::new();
    writeln!(source, "pub const WIDTH: usize = {width};\npub const LENGTH: usize = {length};\npub const LAYERS: usize = {};", stack.layers().len()).unwrap();
    writeln!(source, "pub const PIVOT_Q8: [i32; 3] = [{x}, {y}, {z}];").unwrap();
    writeln!(source, "pub const PALETTE: &[[u8; 5]] = &{colors:?};").unwrap();
    full.write(&mut source, "");
    coarse.write(&mut source, "COARSE_");
    std::fs::write(output.join("pixel_flight_jet.rs"), source).map_err(|e| e.to_string())
}
