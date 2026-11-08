use crate::render::texture::{ColorFormat, Texture};

fn stamp(bytes: &'static [u8]) -> Texture<'static> {
    Texture::from_static(bytes, 128, 128, ColorFormat::RGBA8888).expect("valid gradient stamp")
}

pub(super) fn dark() -> Texture<'static> {
    stamp(include_bytes!("assets/dark.rgba"))
}
pub(super) fn light() -> Texture<'static> {
    stamp(include_bytes!("assets/light.rgba"))
}
pub(super) fn blue() -> Texture<'static> {
    stamp(include_bytes!("assets/blue.rgba"))
}
pub(super) fn amber() -> Texture<'static> {
    stamp(include_bytes!("assets/amber.rgba"))
}
