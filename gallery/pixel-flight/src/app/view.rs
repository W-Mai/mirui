use super::input::{FlightViewState, cancel, press, release};
use crate::{
    Profile,
    flight::View as CameraView,
    gameplay::FlightModel,
    presentation::{cockpit::cockpit_layer, frame::flight_content},
};
use core::cell::Cell;
use mirui::prelude::*;
use mirui::render::font::{Font, FontManager, FontToken};
use mirui::ui::widgets::{ParagraphStyle, Text, TextAlign};
use mirui_sprite_stack::StackViewport;

pub(super) const FLIGHT_FONT: FontToken = FontToken::Custom("pixel-flight-ui");

#[mirui::compose]
pub(super) fn build_widgets(model: model!(FlightModel), profile: Profile) {
    if profile == Profile::Desktop {
        let font = Font::from_mirx(
            "Flight UI",
            12,
            include_bytes!("../../../../src/gallery/demos/assets/nearby-menu.mirx"),
            &mirx::reader::PayloadLimits::HOST,
        )
        .expect("bundled pixel UI font");
        res!(FontManager).with(|fonts| fonts.add_static(FLIGHT_FONT.cache_key(), font));
    }
    if !res!(optional MonoClock).with(|clock| clock.is_some()) {
        res!().insert(MonoClock::from_time_source());
    }
    let content = flight_content(model.clone(), profile);
    let cockpit = cockpit_layer(model.clone(), profile);

    ui! {
        StackViewport (
            stage: model.stage(),
            resolution: profile.resolution(),
            grow: 1.0,
            clip_children: true,
            direction: FlexDirection::Column,
            justify: JustifyContent::FlexEnd,
            padding: Padding {
                bottom: Dimension::px(if profile == Profile::Compact { 2 } else { 18 }),
                left: Dimension::px(if profile == Profile::Compact { 2 } else { 18 }),
                right: Dimension::px(if profile == Profile::Compact { 2 } else { 18 }),
                ..Padding::all(0)
            }
        ) [
            FlightViewState {
                model: model.clone(),
                observed_members: Cell::new(0),
            },
            content,
            cockpit,
            mirui::input::event::scroll::TouchAction::None,
        ] {
            Text (
                ${ hint(model.view(), profile, model.autoplay()) },
                width: Dimension::percent(100),
                font: if profile == Profile::Compact { FontToken::Default } else { FLIGHT_FONT },
                font_size: if profile == Profile::Compact { 8 } else { 12 },
                text_color: ${ model.hint_color() },
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            ) [
                mirui::ui::IgnoreHitTest,
            ]
        } on Pressed(press::callback(model.clone())) on Released(release::callback(model.clone())) on PressCancelled(cancel::callback(model))
    };
}
fn hint(view: CameraView, profile: Profile, autoplay: bool) -> &'static str {
    if autoplay {
        return match (view, profile) {
            (CameraView::Overhead, Profile::Compact) => "AUTO / TOP",
            (CameraView::Chase, Profile::Compact) => "AUTO / CHASE",
            (CameraView::Cockpit, Profile::Compact) => "AUTO / COCKPIT",
            (CameraView::Overhead, Profile::Desktop) => "AUTO FLIGHT / OVERHEAD / CAMERA EVERY 12S",
            (CameraView::Chase, Profile::Desktop) => "AUTO FLIGHT / CHASE / CAMERA EVERY 12S",
            (CameraView::Cockpit, Profile::Desktop) => "AUTO FLIGHT / COCKPIT / CAMERA EVERY 12S",
        };
    }
    if profile == Profile::Compact {
        return match view {
            CameraView::Overhead => "TOP / SPACE",
            CameraView::Chase => "CHASE / SPACE",
            CameraView::Cockpit => "COCKPIT / SPACE",
        };
    }
    match view {
        CameraView::Overhead => {
            "MOVE TO STEER · CLICK TO ROLL · HOLD / SHIFT TO BOOST · SPACE FOR CHASE VIEW"
        }
        CameraView::Chase => {
            "POINT TO TURN, CLIMB AND DIVE · CLICK TO ROLL · HOLD / SHIFT TO BOOST · SPACE FOR COCKPIT"
        }
        CameraView::Cockpit => {
            "ARROWS / WASD TO FLY · CLICK TO ROLL · HOLD / SHIFT TO BOOST · SPACE FOR OVERHEAD"
        }
    }
}
