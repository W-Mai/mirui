//! Multi-representation bundle demo — one row where each character is
//! a step larger than the last, all from a single bundled `Font`. Small
//! sizes resolve to baked pixel tables (crisp 1-bit, 10px then 12px);
//! once a size outgrows the largest pixel table it falls to the SDF
//! table, scaled to the request. The caller only sets a size per glyph;
//! the bundle picks the representation.

extern crate alloc;

use crate::prelude::*;
use crate::render::font::{FontManager, multi};
use crate::ui::widgets::Text;

const BUNDLE: &[u8] = include_bytes!("assets/multi_font_bundle.mirx");

// One glyph per ascending size. 10/12 land on the pixel tables; the
// rest outgrow them and resolve to the scaled SDF.
const STEPS: [(&str, u16); 6] = [
    ("你", 10),
    ("好", 12),
    ("世", 16),
    ("界", 22),
    ("文", 32),
    ("字", 44),
];

fn token(size: u16) -> FontToken {
    match size {
        10 => FontToken::Custom("step10"),
        12 => FontToken::Custom("step12"),
        16 => FontToken::Custom("step16"),
        22 => FontToken::Custom("step22"),
        32 => FontToken::Custom("step32"),
        _ => FontToken::Custom("step44"),
    }
}

pub fn register_font(world: &mut World) {
    let Some(mgr) = world.resource::<FontManager>() else {
        return;
    };
    for (_, size) in STEPS {
        let mut font = multi::font_from_mirx_bundle("Bundle", BUNDLE).expect("bundle parses");
        font.size = size;
        mgr.add_static(token(size).cache_key(), font);
    }
}

pub fn build_widgets(world: &mut World, parent: Entity) {
    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        Row (
            grow: 1.0,
            padding: Padding::all(8),
            justify: JustifyContent::SpaceEvenly,
            align: AlignItems::Center
        ) {
            walk STEPS.iter() with step {
                Text (
                    step.0,
                    font: token(step.1),
                    text_color: Color::rgb(255, 210, 130)
                )
            }
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
    fn bundle_font_reports_a_pixel_default_size() {
        let font = multi::font_from_mirx_bundle("Bundle", BUNDLE).unwrap();
        assert_eq!(font.size, 12);
    }

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(crate::render::font::default_font_manager());
        register_font(&mut world);
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        let row = world
            .get::<Children>(parent)
            .and_then(|c| c.0.first().copied())
            .expect("demo did not add a row to parent");
        assert!(
            world
                .get::<Children>(row)
                .is_some_and(|c| c.0.len() == STEPS.len()),
            "row did not hold one Text per step",
        );
    }
}
