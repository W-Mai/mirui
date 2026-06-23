//! A slice-backed [`SceneResolver`] covering both reference modes: `Index`
//! picks the nth resource (the embedded mirx chunk order), `Token` looks one
//! up by name (runtime resource table).

use super::ResourceRef;
use super::replay::SceneResolver;
use crate::render::font::Font;
use crate::render::texture::Texture;

pub struct SliceResolver<'a> {
    pub fonts: &'a [(&'a str, &'a Font)],
    pub textures: &'a [(&'a str, &'a Texture<'a>)],
}

impl<'a> SliceResolver<'a> {
    pub const fn new(
        fonts: &'a [(&'a str, &'a Font)],
        textures: &'a [(&'a str, &'a Texture<'a>)],
    ) -> Self {
        Self { fonts, textures }
    }
}

impl SceneResolver for SliceResolver<'_> {
    fn font(&self, r: &ResourceRef) -> Option<&Font> {
        match r {
            ResourceRef::Index(i) => self.fonts.get(*i as usize).map(|(_, f)| *f),
            ResourceRef::Token(name) => self
                .fonts
                .iter()
                .find(|(n, _)| *n == name.as_ref())
                .map(|(_, f)| *f),
        }
    }

    fn texture(&self, r: &ResourceRef) -> Option<&Texture<'_>> {
        match r {
            ResourceRef::Index(i) => self.textures.get(*i as usize).map(|(_, t)| *t),
            ResourceRef::Token(name) => self
                .textures
                .iter()
                .find(|(n, _)| *n == name.as_ref())
                .map(|(_, t)| *t),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::command::DrawCommand;
    use crate::render::scene::SceneOp;
    use crate::render::scene::codec::encode_scene;
    use crate::render::scene::replay::replay_scene;
    use crate::render::texture::{ColorFormat, Texture};
    use crate::types::{Point, Rect, Transform};
    use alloc::vec;

    struct DimsRenderer {
        sizes: alloc::vec::Vec<(u16, u16)>,
    }
    impl crate::render::renderer::Renderer for DimsRenderer {
        fn draw(&mut self, cmd: &DrawCommand, _clip: &Rect) {
            if let DrawCommand::Blit { texture, .. } = cmd {
                self.sizes.push((texture.width, texture.height));
            }
        }
        fn flush(&mut self) {}
    }

    static PIXELS_A: [u8; 4] = [1, 2, 3, 4];
    static PIXELS_B: [u8; 8] = [0; 8];

    fn blit(ref_: ResourceRef) -> SceneOp {
        SceneOp::Blit {
            texture: ref_,
            pos: Point::ZERO,
            size: Point::ZERO,
            transform: Transform::IDENTITY,
            quad: None,
        }
    }

    #[test]
    fn index_and_token_modes_resolve_distinct_textures() {
        let tex_a = Texture::from_static(&PIXELS_A, 1, 1, ColorFormat::RGBA8888);
        let tex_b = Texture::from_static(&PIXELS_B, 2, 1, ColorFormat::RGBA8888);
        let textures: [(&str, &Texture); 2] = [("a", &tex_a), ("b", &tex_b)];
        let fonts: [(&str, &Font); 0] = [];
        let resolver = SliceResolver::new(&fonts, &textures);

        let ops = vec![
            blit(ResourceRef::Index(1)),
            blit(ResourceRef::Token("a".into())),
        ];
        let mut r = DimsRenderer { sizes: vec![] };
        replay_scene(&ops, &mut r, &Rect::ZERO, &resolver).unwrap();
        assert_eq!(r.sizes, vec![(2, 1), (1, 1)]);
    }

    #[test]
    fn vector_image_font_coexist_in_one_file() {
        use mirx::{ChunkEntry, chunk_type, encode_chunks, parse_chunk};

        let vector = encode_scene(&[blit(ResourceRef::Index(0))]).unwrap();
        let image: &[u8] = &[0xAA, 0xBB, 0xCC, 0xDD];
        let font: &[u8] = &[0x01, 0x00, 0x10, 0x00];

        let chunks: alloc::vec::Vec<(u16, u16, &[u8])> = vec![
            (
                chunk_type::VECTOR,
                ChunkEntry::FLAG_CRITICAL,
                vector.as_slice(),
            ),
            (chunk_type::IMAGE, ChunkEntry::FLAG_CRITICAL, image),
            (chunk_type::FONT, ChunkEntry::FLAG_CRITICAL, font),
        ];
        let bytes = encode_chunks(&chunks);
        let parsed = parse_chunk(&bytes).unwrap();

        assert_eq!(
            parsed.chunk_payload(&bytes, chunk_type::VECTOR).unwrap(),
            vector.as_slice()
        );
        assert_eq!(
            parsed.chunk_payload(&bytes, chunk_type::IMAGE).unwrap(),
            image
        );
        assert_eq!(
            parsed.chunk_payload(&bytes, chunk_type::FONT).unwrap(),
            font
        );
    }
}
