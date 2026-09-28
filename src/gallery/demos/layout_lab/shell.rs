use super::cards::{
    compose_collection_card, compose_flex_card, compose_header, compose_overlay_card,
    compose_surface_card,
};
use super::style::BACKGROUND;
use crate::prelude::*;

#[compose]
pub fn build_widgets() {
    //~focus-start
    ui! {
        Scroll (
            id: "layout_lab_shell",
            grow: 1.0,
            clip_children: true,
            bg_color: BACKGROUND
        ) {
            Column (
                id: "layout_lab_document",
                width: Dimension::percent(100),
                height: Dimension::Content,
                min_height: Dimension::percent(100),
                padding: Padding::all(18),
                row_gap: 12
            ) {
                compose_header ()
                Row (
                    id: "layout_lab_grid",
                    width: Dimension::percent(100),
                    height: Dimension::Content,
                    wrap: FlexWrap::Wrap,
                    align: AlignItems::FlexStart,
                    row_gap: 12,
                    column_gap: 12
                ) {
                    compose_flex_card ()
                    compose_surface_card ()
                    compose_overlay_card ()
                    compose_collection_card ()
                }
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
    crate::gallery::showcase_theme::install(&mut app.world);
    app.add_plugin(crate::app::plugins::ImageResourcesPlugin::default());
    app.compose(parent, build_widgets);
}
