use super::{AlphaMode, ColorFormat, blend_straight_source_over};
use crate::types::Color;

fn blend_opaque(source: &Color, destination: &Color, alpha: u8) -> Color {
    let alpha = u32::from(alpha);
    let inverse = 255 - alpha;
    let source_rb = u32::from(source.r) | (u32::from(source.b) << 16);
    let destination_rb = u32::from(destination.r) | (u32::from(destination.b) << 16);
    // Each 16-bit lane remains <= 65406 through the original rounding formula.
    let rb_sum = source_rb * alpha + destination_rb * inverse + 0x007F_007F;
    let rb = (rb_sum + ((rb_sum >> 8) & 0x00FF_00FF)) >> 8;
    let g_sum = u32::from(source.g) * alpha + u32::from(destination.g) * inverse + 127;
    let g = ((g_sum + (g_sum >> 8)) >> 8) as u8;
    Color::rgb(rb as u8, g, (rb >> 16) as u8)
}

#[inline(always)]
pub(super) fn pixel_offset(
    width: u16,
    height: u16,
    stride: usize,
    format: ColorFormat,
    x: i32,
    y: i32,
) -> Option<usize> {
    if x < 0 || y < 0 || x >= i32::from(width) || y >= i32::from(height) {
        return None;
    }
    Some(y as usize * stride + x as usize * format.bytes_per_pixel())
}

impl ColorFormat {
    /// Whether arbitrary-alpha fixed-source rows use the byte-channel kernel.
    pub(crate) fn can_batch_solid(self, mode: AlphaMode) -> bool {
        mode == AlphaMode::Opaque && matches!(self, Self::RGBA8888 | Self::BGRA8888 | Self::RGB888)
    }

    /// Blend complete pixels with a fixed source and effective alpha.
    pub(crate) fn blend_solid_span(
        self,
        bytes: &mut [u8],
        color: &Color,
        alpha: u8,
        mode: AlphaMode,
    ) {
        if alpha == 0 {
            return;
        }
        if self.can_batch_solid(mode) || alpha == 255 {
            match self {
                Self::RGBA8888 => {
                    blend_constant_bytes::<4>(bytes, [color.r, color.g, color.b], alpha);
                    return;
                }
                Self::BGRA8888 => {
                    blend_constant_bytes::<4>(bytes, [color.b, color.g, color.r], alpha);
                    return;
                }
                Self::RGB888 => {
                    blend_constant_bytes::<3>(bytes, [color.r, color.g, color.b], alpha);
                    return;
                }
                Self::RGB565 | Self::RGB565Swapped => {}
            }
        }
        let size = self.bytes_per_pixel();
        for pixel in bytes.chunks_exact_mut(size) {
            self.blend_pixel(pixel, 0, color, alpha, mode);
        }
    }

    #[inline]
    pub(crate) fn blend_pixel(
        self,
        bytes: &mut [u8],
        offset: usize,
        color: &Color,
        alpha: u8,
        alpha_mode: AlphaMode,
    ) {
        if alpha == 0 {
            return;
        }
        if alpha == 255 {
            self.write_pixel(bytes, offset, &Color { a: 255, ..*color });
            return;
        }
        let previous = self.read_pixel(bytes, offset);
        let result = match alpha_mode {
            AlphaMode::Opaque => blend_opaque(color, &previous, alpha),
            AlphaMode::Blend => blend_straight_source_over(color, &previous, alpha),
        };
        self.write_pixel(bytes, offset, &result);
    }

    #[inline(always)]
    pub(super) fn read_pixel(self, bytes: &[u8], offset: usize) -> Color {
        match self {
            Self::RGBA8888 => Color::rgba(
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ),
            Self::BGRA8888 => Color::rgba(
                bytes[offset + 2],
                bytes[offset + 1],
                bytes[offset],
                bytes[offset + 3],
            ),
            Self::RGB888 => Color::rgb(bytes[offset], bytes[offset + 1], bytes[offset + 2]),
            Self::RGB565 | Self::RGB565Swapped => {
                let pair = [bytes[offset], bytes[offset + 1]];
                let pixel = if self == Self::RGB565 {
                    u16::from_le_bytes(pair)
                } else {
                    u16::from_be_bytes(pair)
                };
                let r = (pixel >> 11) as u8;
                let g = ((pixel >> 5) & 0x3f) as u8;
                let b = (pixel & 0x1f) as u8;
                Color::rgb(
                    (r << 3) | (r >> 2),
                    (g << 2) | (g >> 4),
                    (b << 3) | (b >> 2),
                )
            }
        }
    }

    #[inline(always)]
    pub(super) fn write_pixel(self, bytes: &mut [u8], offset: usize, color: &Color) {
        match self {
            Self::RGB565 | Self::RGB565Swapped => {
                let packed = self.pack(color).to_le_bytes();
                bytes[offset..offset + 2].copy_from_slice(&packed[..2]);
            }
            Self::RGB888 => {
                bytes[offset..offset + 3].copy_from_slice(&[color.r, color.g, color.b]);
            }
            Self::RGBA8888 => {
                bytes[offset..offset + 4].copy_from_slice(&[color.r, color.g, color.b, color.a]);
            }
            Self::BGRA8888 => {
                bytes[offset..offset + 4].copy_from_slice(&[color.b, color.g, color.r, color.a]);
            }
        }
    }

    pub(super) fn copy_row_from(self, destination: &mut [u8], source: Self, samples: &[u8]) {
        match (source, self) {
            (a, b) if a == b => destination.copy_from_slice(samples),
            (Self::RGBA8888, Self::RGB565) => copy_opaque_to_565::<false>(samples, destination),
            (Self::RGBA8888, Self::RGB565Swapped) => {
                copy_opaque_to_565::<true>(samples, destination);
            }
            _ => {
                let src_bpp = source.bytes_per_pixel();
                let dst_bpp = self.bytes_per_pixel();
                for offset in (0..samples.len()).step_by(src_bpp) {
                    let color = source.read_pixel(samples, offset);
                    self.write_pixel(destination, offset / src_bpp * dst_bpp, &color);
                }
            }
        }
    }
}

fn copy_opaque_to_565<const SWAPPED: bool>(source: &[u8], destination: &mut [u8]) {
    let format = if SWAPPED {
        ColorFormat::RGB565Swapped
    } else {
        ColorFormat::RGB565
    };
    for (rgba, out) in source.chunks_exact(4).zip(destination.chunks_exact_mut(2)) {
        out.copy_from_slice(
            &(format.pack(&Color::rgb(rgba[0], rgba[1], rgba[2])) as u16).to_le_bytes(),
        );
    }
}

fn blend_constant_bytes<const BPP: usize>(bytes: &mut [u8], color: [u8; 3], alpha: u8) {
    if alpha == 255 {
        for pixel in bytes.chunks_exact_mut(BPP) {
            pixel[..3].copy_from_slice(&color);
            if BPP == 4 {
                pixel[3] = 255;
            }
        }
        return;
    }
    let alpha = u16::from(alpha);
    let inverse = 255 - alpha;
    let source = color.map(|channel| u16::from(channel) * alpha + 127);
    for pixel in bytes.chunks_exact_mut(BPP) {
        for channel in 0..3 {
            let sum = u16::from(pixel[channel]) * inverse + source[channel];
            pixel[channel] = ((sum + (sum >> 8)) >> 8) as u8;
        }
        if BPP == 4 {
            pixel[3] = 255;
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/render/texture/pixels.rs"]
mod tests;
