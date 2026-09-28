use alloc::vec;

use super::composition::build_widgets;
use crate::anim::ease;
use crate::input::event::sim::{SimAction, SimTimeline};
use crate::prelude::*;
use crate::types::DimPoint;

pub fn install<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.compose(parent, build_widgets);
}

pub(super) fn automation(world: &World) -> Option<SimTimeline> {
    let tabs = world.find_by_id("compact_widgets_tabs")?;
    let list = world.find_by_id("compact_widgets_list")?;
    let slider = world.find_by_id("compact_widgets_slider")?;
    let switch = world.find_by_id("compact_widgets_switch")?;
    let light = world.find_by_id("compact_theme_light")?;
    let dark = world.find_by_id("compact_theme_dark")?;

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
    if let Some(timeline) = automation(&app.world) {
        app.world.insert_resource(timeline);
    }
}
