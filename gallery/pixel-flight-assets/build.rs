use mirui_sprite_stack_format::StackDescriptor;
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

// Preserve the dominant material in a 2x2 cell, resolving ties toward the higher material.
fn coarse_color(samples: [u8; 4]) -> u8 {
    let mut counts = [0u16; 4];
    let mut sums = [0u16; 4];
    for color in samples.into_iter().filter(|&c| c != 0) {
        let index = material(color);
        counts[index] += 1;
        sums[index] += u16::from(color);
    }
    let (index, &count) = counts
        .iter()
        .enumerate()
        .max_by_key(|&(_, count)| count)
        .unwrap();
    (sums[index] + count / 2).checked_div(count).unwrap_or(0) as u8
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
    let stack = StackDescriptor::from_reader(&reader).map_err(|e| e.to_string())?;
    let (width, length) = stack.canvas();
    if width > 256 || length > 256 || width % 2 != 0 || length % 2 != 0 || stack.spacing_q8() != 256
    {
        return Err(
            "aircraft LOD requires even dimensions up to 256 and unit layer spacing".into(),
        );
    }
    let (width, length) = (usize::from(width), usize::from(length));
    let image = reader
        .primary()
        .map_err(|e| format!("{e:?}"))?
        .ok_or("missing atlas")?
        .image()
        .map_err(|e| format!("{e:?}"))?
        .and_then(|image| image.raw())
        .ok_or("expected a RAW atlas")?;
    if image.surface().sample_layout() != SampleLayout::I8 {
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

    let plane = image.plane(0).ok_or("missing atlas plane")?;
    let mut cells = vec![0; width * length];
    let mut full = Level::default();
    let mut coarse = Level::default();
    for region in stack.layers() {
        if region.width() != 0 {
            for (y, row) in cells.chunks_exact_mut(width).enumerate() {
                let atlas_row = plane
                    .row(region.y() + y as u32)
                    .map_err(|e| format!("{e:?}"))?
                    .ok_or("missing atlas row")?;
                row.copy_from_slice(&atlas_row[region.x() as usize..region.right() as usize]);
                for (x, &color) in row.iter().enumerate() {
                    if color != 0 && !opaque[usize::from(color)] {
                        return Err(format!("missing material at palette index {color}"));
                    }
                    full.sample(x, y, color);
                }
            }
            for y in (0..length).step_by(2) {
                for x in (0..width).step_by(2) {
                    let at = y * width + x;
                    let color = coarse_color([
                        cells[at],
                        cells[at + 1],
                        cells[at + width],
                        cells[at + width + 1],
                    ]);
                    if color != 0 && !opaque[usize::from(color)] {
                        return Err(format!("missing coarse LOD shade at palette index {color}"));
                    }
                    coarse.sample(x, y, color);
                }
            }
        }
        full.finish_layer();
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
