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
#[path = "../../../tests/unit/render/scene/resolver.rs"]
mod tests;
