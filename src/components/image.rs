use crate::draw::texture::Texture;

/// Image component — references a Texture
pub struct Image {
    pub texture: &'static Texture<'static>,
}

impl Image {
    pub fn new(texture: &'static Texture<'static>) -> Self {
        Self { texture }
    }
}
