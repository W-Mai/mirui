use alloc::vec;

use super::composition::build_widgets;
use crate::anim::ease;
use crate::input::event::sim::{SimAction, SimTimeline};
use crate::prelude::*;
use crate::types::DimPoint;
use crate::ui::IdMap;

pub fn install<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.compose(parent, build_widgets());
}

#[compose]
pub(super) fn automation() -> Option<SimTimeline> {
    let (tabs, list, slider, switch, light, dark) = res!(IdMap).with(|ids| {
        Some((
            ids.get("compact_widgets_tabs")?,
            ids.get("compact_widgets_list")?,
            ids.get("compact_widgets_slider")?,
            ids.get("compact_widgets_switch")?,
            ids.get("compact_theme_light")?,
            ids.get("compact_theme_dark")?,
        ))
    })?;

    Some(
        SimTimeline::new(vec![
            SimAction::wait(500),
            SimAction::drag(
                DimPoint::percent(50, 80),
                DimPoint::percent(50, 20),
                700,
                ease::ease_in_out_cubic,
            )
            .on(list),
            SimAction::wait(500),
            SimAction::tap(DimPoint::percent(50, 50)).on(tabs),
            SimAction::wait(400),
            SimAction::drag(
                DimPoint::percent(15, 50),
                DimPoint::percent(85, 50),
                700,
                ease::ease_in_out_cubic,
            )
            .on(slider),
            SimAction::wait(400),
            SimAction::tap(DimPoint::CENTER).on(switch),
            SimAction::wait(700),
            SimAction::tap(DimPoint::percent(83, 50)).on(tabs),
            SimAction::wait(500),
            SimAction::tap(DimPoint::CENTER).on(light),
            SimAction::wait(1_800),
            SimAction::tap(DimPoint::CENTER).on(dark),
            SimAction::wait(1_800),
            SimAction::tap(DimPoint::percent(16, 50)).on(tabs),
            SimAction::wait(400),
        ])
        .looping(true),
    )
}

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    install(app, parent);
    app.add_system(crate::input::event::sim::sim_timeline_system::system());
    #[cfg(feature = "std")]
    if std::env::var("MIRUI_SIM_OFF").ok().as_deref() == Some("1") {
        return;
    }
    if let Some(timeline) = app.compose(parent, automation()) {
        app.add_resource(timeline);
    }
}
