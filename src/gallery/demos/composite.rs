extern crate alloc;

use crate::prelude::*;
use crate::render::command::CompositeMode;
use crate::ui::widgets::{Image, ParagraphStyle, Text};

const MODES: &[(&str, CompositeMode)] = &[
    ("source-over", CompositeMode::SourceOver),
    ("add", CompositeMode::Add),
    ("screen", CompositeMode::Screen),
    ("multiply", CompositeMode::Multiply),
    ("darken", CompositeMode::Darken),
    ("lighten", CompositeMode::Lighten),
    ("difference", CompositeMode::Difference),
];

const CARD_W: i32 = 88;
const CARD_H: i32 = 110;
const CELL_W: i32 = 76;
const CELL_H: i32 = 76;
const FG: i32 = 52;

#[compose]
pub fn build_widgets() {
    ui! {
        Column (
            grow: 1.0,
            bg_color: ColorToken::Surface,
            padding: Padding::all(12),
            align: AlignItems::Center
        ) {
            Text (
                "Composite modes",
                width: Dimension::percent(100),
                height: 30,
                font_size: 20,
                text_color: ColorToken::OnSurface
            )
            Text (
                "Each tile blends the same image over red, green, blue, and black bands.",
                width: Dimension::percent(100),
                height: 38,
                text_color: ColorToken::OnSurfaceVariant
            )
            Row (
                id: "composite_grid",
                width: Dimension::percent(100),
                max_width: 760,
                justify: JustifyContent::Center,
                align: AlignItems::FlexStart,
                wrap: FlexWrap::Wrap,
                grow: 1.0,
                row_gap: 8,
                column_gap: 8
            ) {
                walk MODES.iter() with cell {
                    Column (
                        width: CARD_W,
                        height: CARD_H,
                        padding: Padding::all(6),
                        align: AlignItems::Center,
                        row_gap: 4,
                        bg_color: ColorToken::SurfaceVariant,
                        border_color: ColorToken::Outline,
                        border_width: 1,
                        border_radius: 10
                    ) {
                        View (
                            width: CELL_W,
                            height: CELL_H,
                            border_radius: 7,
                            direction: FlexDirection::Row,
                            clip_children: true
                        ) {
                            View (
                                bg_color: Color::rgb(220, 40, 40),
                                width: CELL_W / 4,
                                height: CELL_H
                            )
                            View (
                                bg_color: Color::rgb(40, 200, 80),
                                width: CELL_W / 4,
                                height: CELL_H
                            )
                            View (
                                bg_color: Color::rgb(60, 110, 230),
                                width: CELL_W / 4,
                                height: CELL_H
                            )
                            View (
                                bg_color: Color::rgb(20, 20, 20),
                                width: CELL_W / 4,
                                height: CELL_H
                            )
                            Image (
                                position: Position::Absolute,
                                left: (CELL_W - FG) / 2,
                                top: (CELL_H - FG) / 2,
                                width: FG,
                                height: FG,
                                src: "thumbs_up",
                                composite: cell.1
                            )
                        }
                        Text (
                            cell.0,
                            width: Dimension::percent(100),
                            height: 18,
                            font_size: 8,
                            text_color: ColorToken::OnSurface,
                            paragraph: ParagraphStyle::label()
                        )
                    }
                }
            }
        }
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: crate::surface::Surface,
    F: crate::app::RendererFactory<B>,
{
    use crate::app::plugins::{ImageResourcesPlugin, StdInstantClockPlugin};
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(ImageResourcesPlugin::default());
    app.compose(parent, build_widgets);
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(720, 360);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Viewport;
    use crate::ui::ComputedRect;
    use crate::ui::render_system::update_layout;

    #[test]
    fn every_composite_mode_has_one_image_sample() {
        let mut app = App::headless(720, 360);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        app.compose(root, build_widgets);

        assert_eq!(app.world.query::<Image>().iter().count(), MODES.len());
    }

    #[test]
    fn samples_wrap_without_leaving_supported_viewports() {
        for (width, height) in [(320, 568), (480, 320), (1024, 640)] {
            let mut app = App::headless(width, height);
            app.with_default_widgets().with_default_systems();
            let root = app.spawn_root().id();
            app.compose(root, build_widgets);
            app.set_root(root);
            update_layout(
                &mut app.world,
                root,
                &Viewport::new(width, height, Fixed::ONE),
            );

            let samples: alloc::vec::Vec<Entity> = app
                .world
                .query::<Image>()
                .iter()
                .map(|(entity, _)| entity)
                .collect();
            assert_eq!(samples.len(), MODES.len());
            for entity in samples {
                let rect = app.world.get::<ComputedRect>(entity).unwrap().0;
                assert!(rect.x >= Fixed::ZERO);
                assert!(rect.y >= Fixed::ZERO);
                assert!(rect.x + rect.w <= Fixed::from_int(width as i32));
                assert!(rect.y + rect.h <= Fixed::from_int(height as i32));
            }
        }
    }

    #[test]
    fn samples_reflow_between_landscape_and_portrait() {
        fn sample_rows(width: u16, height: u16) -> usize {
            let mut app = App::headless(width, height);
            app.with_default_widgets().with_default_systems();
            let root = app.spawn_root().id();
            app.compose(root, build_widgets);
            app.set_root(root);
            update_layout(
                &mut app.world,
                root,
                &Viewport::new(width, height, Fixed::ONE),
            );

            let mut rows: alloc::vec::Vec<i32> = app
                .world
                .query::<Image>()
                .iter()
                .filter_map(|(entity, _)| app.world.get::<ComputedRect>(entity))
                .map(|rect| rect.0.y.round().to_int())
                .collect();
            rows.sort_unstable();
            rows.dedup();
            rows.len()
        }

        assert_eq!(sample_rows(720, 360), 1);
        assert!(sample_rows(360, 720) > 1);
    }
}
