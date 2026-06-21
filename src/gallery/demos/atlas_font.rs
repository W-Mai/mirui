//! Multi-size font demo — three lines, each from a different bundled
//! mirx atlas, showing how representation follows size:
//!
//! - 10px / 12px lines use 1-bit pixel fonts baked at their design
//!   size, so strokes land on whole pixels and stay crisp.
//! - the 24px line uses an SDF atlas, which scales one source to the
//!   target without the thin-stem softening that hurts SDF at tiny
//!   sizes.
//!
//! Each font registers under its own [`FontToken`] and renders at its
//! own design size; [`register_font`] must run before [`build_widgets`].

extern crate alloc;

use crate::prelude::*;
use crate::render::font::{Font, FontManager, gray, sdf};
use crate::ui::widgets::Text;

const PIXEL_10: &[u8] = include_bytes!("assets/fusion_pixel_10_1bit.mirx");
const PIXEL_12: &[u8] = include_bytes!("assets/fusion_pixel_12_1bit.mirx");
const SDF_24: &[u8] = include_bytes!("assets/misans_sdf_24.mirx");

const TOKEN_10: FontToken = FontToken::Custom("pixel10");
const TOKEN_12: FontToken = FontToken::Custom("pixel12");
const TOKEN_24: FontToken = FontToken::Custom("sdf24");

fn font_payload(atlas: &'static [u8]) -> &'static [u8] {
    mirx::parse_chunk(atlas)
        .expect("bundled atlas parses")
        .chunk_payload(atlas, mirx::chunk_type::FONT)
        .expect("FONT chunk present")
}

/// Register the three demo fonts in the world's [`FontManager`], each
/// under its own token. Idempotent — re-registering rebinds the keys.
pub fn register_font(world: &mut World) {
    let Some(mgr) = world.resource::<FontManager>() else {
        return;
    };
    let pixel10: Font =
        gray::font_from_mirx_chunk("FusionPixel-10", font_payload(PIXEL_10)).expect("10px atlas");
    let pixel12: Font =
        gray::font_from_mirx_chunk("FusionPixel-12", font_payload(PIXEL_12)).expect("12px atlas");
    let sdf24: Font =
        sdf::font_from_mirx_chunk("MiSans-SDF-24", font_payload(SDF_24)).expect("24px atlas");
    mgr.add_static(TOKEN_10.cache_key(), pixel10);
    mgr.add_static(TOKEN_12.cache_key(), pixel12);
    mgr.add_static(TOKEN_24.cache_key(), sdf24);
}

pub fn build_widgets(world: &mut World, parent: Entity) {
    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (
            grow: 1.0,
            padding: Padding::all(8),
            justify: JustifyContent::SpaceEvenly,
            align: AlignItems::Center
        ) {
            Text (
                "10px 像素",
                font: FontToken::Custom("pixel10"),
                text_color: Color::rgb(160, 255, 180)
            )
            Text (
                "12px 像素",
                font: FontToken::Custom("pixel12"),
                text_color: Color::rgb(140, 220, 255)
            )
            Text (
                "24px SDF",
                font: FontToken::Custom("sdf24"),
                text_color: Color::rgb(255, 200, 120)
            )
        }
    };
    //~focus-end
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    register_font(&mut app.world);
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;

    #[test]
    fn loads_three_atlases_at_their_sizes() {
        let p10 = gray::font_from_mirx_chunk("p10", font_payload(PIXEL_10)).unwrap();
        let p12 = gray::font_from_mirx_chunk("p12", font_payload(PIXEL_12)).unwrap();
        let s24 = sdf::font_from_mirx_chunk("s24", font_payload(SDF_24)).unwrap();
        assert_eq!(p10.size, 10);
        assert_eq!(p12.size, 12);
        assert_eq!(s24.size, 24);
    }

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(crate::render::font::default_font_manager());
        register_font(&mut world);
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
            "demo did not add any children to parent",
        );
    }
}
