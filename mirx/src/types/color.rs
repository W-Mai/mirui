//! Byte-order RGBA colour. `{r, g, b, a}` — byte 0 = R, matching
//! `mirui::types::Color` and every backend mirui ships today (SDL
//! RGBA32, wgpu Rgba8Unorm, WebGL / Canvas ImageData).
//!
//! Wire layout: 4 bytes `[r, g, b, a]`. This is the same layout the
//! existing `mirui::render::scene::codec` writes, so old `.mirx`
//! bytes decode identically through `mirx::Color`.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}
