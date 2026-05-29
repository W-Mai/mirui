//! Pre-built glyph atlas for the 8×8 bitmap font. 95 printable ASCII
//! glyphs unpacked once at first use into a 128×48 R8 texture (16×6
//! grid, 8 px per cell). No LRU or runtime packing — the font is
//! fixed size and small enough to live entirely in the atlas.

use crate::draw::font::{CHAR_H, CHAR_W, glyph};

pub const COLS: u32 = 16;
pub const ROWS: u32 = 6;
pub const ATLAS_W: u32 = COLS * CHAR_W;
pub const ATLAS_H: u32 = ROWS * CHAR_H;

pub struct GlyphAtlas {
    pub texture: wgpu::Texture,
}

impl GlyphAtlas {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let pixels = unpack_atlas();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mirui-glyph-atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_W,
                height: ATLAS_H,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ATLAS_W),
                rows_per_image: Some(ATLAS_H),
            },
            wgpu::Extent3d {
                width: ATLAS_W,
                height: ATLAS_H,
                depth_or_array_layers: 1,
            },
        );
        Self { texture }
    }

    /// `[u0, v0, u1, v1]` UV rect for a character. Falls through to '?'
    /// for code points outside 32..127, matching `font::glyph`.
    pub fn uv_for(ch: u8) -> [f32; 4] {
        let idx = if (32..127).contains(&ch) {
            (ch - 32) as u32
        } else {
            (b'?' - 32) as u32
        };
        let col = idx % COLS;
        let row = idx / COLS;
        let u0 = (col * CHAR_W) as f32 / ATLAS_W as f32;
        let v0 = (row * CHAR_H) as f32 / ATLAS_H as f32;
        let u1 = ((col + 1) * CHAR_W) as f32 / ATLAS_W as f32;
        let v1 = ((row + 1) * CHAR_H) as f32 / ATLAS_H as f32;
        [u0, v0, u1, v1]
    }
}

fn unpack_atlas() -> alloc::vec::Vec<u8> {
    let mut out = alloc::vec![0u8; (ATLAS_W * ATLAS_H) as usize];
    for ch in 32u8..127 {
        let idx = (ch - 32) as u32;
        let col = idx % COLS;
        let row = idx / COLS;
        let cell_x = col * CHAR_W;
        let cell_y = row * CHAR_H;
        let bitmap = glyph(ch);
        for ry in 0..CHAR_H {
            let byte = bitmap[ry as usize];
            for cx in 0..CHAR_W {
                let on = (byte & (0x80 >> cx)) != 0;
                let px = cell_x + cx;
                let py = cell_y + ry;
                let i = (py * ATLAS_W + px) as usize;
                out[i] = if on { 0xff } else { 0x00 };
            }
        }
    }
    out
}
