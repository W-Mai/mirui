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
            ResourceRef::Inline(_) => None,
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
            ResourceRef::Inline(_) => None,
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

    fn encode_chunks(chunks: &[(u16, u16, &[u8])]) -> alloc::vec::Vec<u8> {
        const HEADER_LEN: usize = 44;
        const ENTRY_LEN: usize = 16;

        let payload_start = HEADER_LEN + chunks.len() * ENTRY_LEN;
        let payload_len: usize = chunks.iter().map(|(_, _, payload)| payload.len()).sum();
        let file_len = payload_start + payload_len;
        let mut bytes = vec![0; file_len];

        bytes[..4].copy_from_slice(b"MIRX");
        bytes[4] = 1;
        bytes[6] = 1;
        bytes[8..10].copy_from_slice(&(chunks.len() as u16).to_le_bytes());
        bytes[12..16].copy_from_slice(&(HEADER_LEN as u32).to_le_bytes());
        bytes[16..20].copy_from_slice(&(file_len as u32).to_le_bytes());
        if let Some((chunk_type, _, _)) = chunks.first() {
            bytes[20..22].copy_from_slice(&chunk_type.to_le_bytes());
        }
        let header_crc = mirx::crc32(&bytes[..40]);
        bytes[40..44].copy_from_slice(&header_crc.to_le_bytes());

        let mut payload_offset = payload_start;
        for (index, (chunk_type, flags, payload)) in chunks.iter().enumerate() {
            let entry = HEADER_LEN + index * ENTRY_LEN;
            bytes[entry..entry + 2].copy_from_slice(&chunk_type.to_le_bytes());
            bytes[entry + 2..entry + 4].copy_from_slice(&flags.to_le_bytes());
            bytes[entry + 4..entry + 8].copy_from_slice(&(payload_offset as u32).to_le_bytes());
            bytes[entry + 8..entry + 12].copy_from_slice(&(payload.len() as u32).to_le_bytes());
            bytes[payload_offset..payload_offset + payload.len()].copy_from_slice(payload);
            payload_offset += payload.len();
        }

        bytes
    }

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
            opa: 255,
            radius: crate::types::Fixed::ZERO,
            composite: crate::render::command::CompositeMode::SourceOver,
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
        use mirx::{ChunkType, Reader};

        let vector = encode_scene(&[blit(ResourceRef::Index(0))]).unwrap();
        let image: &[u8] = &[0xAA, 0xBB, 0xCC, 0xDD];
        let font: &[u8] = &[0x01, 0x00, 0x10, 0x00];

        let chunks: alloc::vec::Vec<(u16, u16, &[u8])> = vec![
            (ChunkType::VECTOR.raw(), 0, vector.as_slice()),
            (ChunkType::IMAGE.raw(), 0, image),
            (ChunkType::FONT.raw(), 0, font),
        ];
        let bytes = encode_chunks(&chunks);
        let reader = Reader::open(&bytes).unwrap();

        assert_eq!(
            reader
                .chunks()
                .find(|chunk| chunk.chunk_type() == ChunkType::VECTOR)
                .unwrap()
                .payload(),
            vector.as_slice()
        );
        assert_eq!(
            reader
                .chunks()
                .find(|chunk| chunk.chunk_type() == ChunkType::IMAGE)
                .unwrap()
                .payload(),
            image
        );
        assert_eq!(
            reader
                .chunks()
                .find(|chunk| chunk.chunk_type() == ChunkType::FONT)
                .unwrap()
                .payload(),
            font
        );
    }
}
