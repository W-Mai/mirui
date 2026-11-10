mod geometry;
mod icons;
mod input;
mod render;
mod state;
#[cfg(test)]
mod tests;
mod textures;

use self::render::{GlassToggle, glass_render};
use self::state::GlassModel;
use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_least(320, 320);
pub const VIEWPORT: (u16, u16) = (760, 520);

/// The switch follows the host theme, judged by how bright its surface is.
#[crate::system(order=Animation)]
fn follow_host(glasses: query!(GlassToggle), theme: res!(Theme)) {
    let surface = theme.resolve(ColorToken::Surface);
    let luma = u32::from(surface.r) * 299 + u32::from(surface.g) * 587 + u32::from(surface.b) * 114;
    let day = luma > 128_000;
    for glass in glasses {
        if glass.model.host_day() != Some(day) {
            glass.model.follow_host(day);
        }
    }
}

#[crate::system(order=Animation)]
fn animate(glasses: query!(GlassToggle), delta: res!(DeltaTimeMs)) {
    for glass in glasses {
        if glass.model.active() {
            glass.model.advance(delta.0);
        }
    }
}

#[crate::compose]
fn build_widgets(model: model!(GlassModel)) {
    ui! {
        View (
            grow: 1.0,
            clip_children: true,
            direction: FlexDirection::Column,
            justify: JustifyContent::FlexEnd,
            padding: Padding {
                bottom: Dimension::px(28),
                ..Padding::all(0)
            }
        ) [
            GlassToggle {
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
        } on Pressed(input::press::callback()) on Released(input::release::callback()) on PressCancelled(input::cancel_press::callback())
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
    app.compose(parent, build_widgets(model));
}
