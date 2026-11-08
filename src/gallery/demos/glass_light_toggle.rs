mod icons;
mod input;
mod render;
mod state;
#[cfg(test)]
mod tests;
mod textures;

use self::render::{GlassSurface, glass_render};
use self::state::GlassModel;
use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_least(320, 320);
pub const VIEWPORT: (u16, u16) = (760, 520);

#[crate::system(order=Animation)]
fn animate(model: model!(GlassModel), delta: res!(DeltaTimeMs)) {
    if model.active() {
        model.advance(delta.0);
    }
}

#[crate::compose]
fn build_widgets(model: model!(GlassModel)) {
    ui! {
        View (
            id: "glass_light_toggle",
            grow: 1.0,
            clip_children: true,
            direction: FlexDirection::Column,
            justify: JustifyContent::FlexEnd,
            padding: Padding {
                bottom: Dimension::px(28),
                ..Padding::all(0)
            }
        ) [
            GlassSurface {
                model: model.clone(),
            },
            crate::input::event::scroll::TouchAction::None,
            crate::input::event::focus::Focusable,
        ] {
            Text (
                "C L I C K   O R   D R A G",
                width: Dimension::percent(100),
                height: 16,
                font_size: 9,
                text_color: ${ model.hint_color() },
                paragraph: ParagraphStyle::label().with_align(TextAlign::Center)
            ) [
                crate::ui::IgnoreHitTest,
            ]
        } on Tap(input::tap::callback(model.clone())) on Pressed(input::press::callback(model.clone())) on Released(input::release::callback(model.clone())) on PressCancelled(input::cancel_press::callback(model.clone())) on DragStart(input::drag_start::callback(model.clone())) on DragMove(input::drag_move::callback(model.clone())) on DragEnd(input::drag_end::callback(model.clone())) on DragCancel(input::drag_cancel::callback(model.clone())) on Key(input::key::callback(model))
    };
}

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    #[cfg(feature = "std")]
    app.add_plugin(crate::app::plugins::StdInstantClockPlugin);
    let model = app.add_model(GlassModel::default()).finish();
    app.with_widget(glass_render::view());
    app.add_system(animate::system(model.clone()));
    app.compose(parent, build_widgets(model));
}
