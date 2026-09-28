use alloc::vec;
use alloc::vec::Vec;

#[cfg(feature = "std")]
use super::binding::slider_to_progress_system;
#[cfg(feature = "std")]
use super::composition::build_widgets;
#[cfg(feature = "std")]
use super::state::{Cycle, ThemeCycleIndex, dark_with_accent};
use crate::anim::ease;
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::input::event::sim::{SimAction, SimTimeline};
#[cfg(feature = "std")]
use crate::prelude::plugin::{FpsSummaryPlugin, InputFeedbackPlugin};
use crate::prelude::*;
use crate::types::DimPoint;
use crate::ui::Children;
use crate::ui::widgets::{LazyList, Slider, Switch, TabBar};

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    let info = app.backend.display_info();
    app.add_plugin(InputFeedbackPlugin::default());
    app.add_plugin(StdInstantClockPlugin);
    app.add_plugin(FpsSummaryPlugin::default());
    app.add_plugin(crate::app::plugins::ImageResourcesPlugin::default());
    app.with_offscreen_pool_budget(512 * 1024);
    app.add_system(crate::input::event::sim::sim_timeline_system::system());
    app.add_system(slider_to_progress_system::system());

    app.world.insert_resource(dark_with_accent());
    let cycle_e = Cycle::install(&mut app.world);
    app.world.insert(cycle_e, ThemeCycleIndex(0));

    app.compose(parent, |cx| build_widgets(cx, info.width, info.height));

    if std::env::var("MIRUI_SIM_OFF").ok().as_deref() == Some("1") {
        return;
    }

    if let Some(timeline) = build_sim_timeline(&app.world) {
        app.world.insert_resource(timeline);
    }
}

/// Construct the demo's looping `SimTimeline` from the live widget tree.
/// Returns `None` when the expected widgets aren't installed (e.g.
/// `build_widgets` skipped) — callers can `insert_resource` the result
/// unconditionally and the sim system stays a no-op without a timeline.
pub fn build_sim_timeline(world: &World) -> Option<SimTimeline> {
    let tab_bars: Vec<Entity> = world.query::<TabBar>().collect();
    let tab_bar_e = *tab_bars.first()?;
    let tabs_kids: Vec<Entity> = world
        .get::<Children>(tab_bar_e)
        .map(|c| c.0.clone())
        .unwrap_or_default();
    if tabs_kids.len() < 3 {
        return None;
    }
    let (tab_list, tab_form, tab_theme) = (tabs_kids[0], tabs_kids[1], tabs_kids[2]);

    let switches: Vec<Entity> = world.query::<Switch>().collect();
    let switch_e = *switches.first()?;
    let sliders: Vec<Entity> = world.query::<Slider>().collect();
    let slider_e = *sliders.first()?;
    let lists: Vec<Entity> = world.query::<LazyList>().collect();
    let list_e = *lists.first()?;

    Some(
        SimTimeline::new(vec![
            SimAction::wait(800),
            SimAction::tap(DimPoint::CENTER).on(tab_form),
            SimAction::wait(800),
            SimAction::tap(DimPoint::CENTER).on(switch_e),
            SimAction::wait(800),
            SimAction::drag(
                DimPoint::percent(10, 50),
                DimPoint::percent(90, 50),
                600,
                ease::ease_in_out_cubic,
            )
            .on(slider_e),
            SimAction::wait(800),
            SimAction::tap(DimPoint::CENTER).on(switch_e),
            SimAction::wait(1500),
            SimAction::tap(DimPoint::CENTER).on(tab_theme),
            SimAction::wait(6500),
            SimAction::tap(DimPoint::CENTER).on(tab_list),
            SimAction::wait(800),
            SimAction::drag(
                DimPoint::percent(50, 80),
                DimPoint::percent(50, 20),
                100,
                ease::linear,
            )
            .on(list_e),
            SimAction::wait(800),
            SimAction::drag(
                DimPoint::percent(50, 20),
                DimPoint::percent(50, 80),
                100,
                ease::linear,
            )
            .on(list_e),
            SimAction::wait(800),
        ])
        .looping(true),
    )
}
