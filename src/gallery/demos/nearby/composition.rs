use super::{
    actor::actor,
    launcher_view::launcher,
    model::{DisplayState, PageKey},
    pages::page,
    simulation::controls,
    style::*,
};
use crate::prelude::*;
use crate::ui::widgets::page_view::{CreatePage, PageView};

#[event]
fn create_page(
    event: event!(CreatePage<PageKey>),
    state: model!(DisplayState),
    metrics: Metrics,
    room: bool,
) -> Entity {
    let mut snapshot = state.snapshot();
    snapshot.page = event.route().page;
    ui! {
        page (&snapshot, state, metrics, room)
    }
}

#[compose]
pub(super) fn build_widgets(state: model!(DisplayState)) {
    ui! {
        Column (grow: 1.0, min_height: 0, bg_color: BACKGROUND) {
            if ${ state.launcher().active() || state.return_to_launcher() } {
                launcher (state.clone(), state.launcher())
            } else {
                page_host (state)
                actor (state, false)
            }
            if ${ state.controls() } {
                controls (state)
            }
        }
    };
}

#[compose]
fn page_host(state: model!(DisplayState)) -> Entity {
    let metrics = state.metrics();
    ui! {
        PageView (
            id: "nearby_pages",
            page: ${ state.page_key() },
            grow: 1.0,
            min_height: 0,
            bg_color: BACKGROUND,
            clip_children: true,
            transition: ${ state.transition() }
        ) on CreatePage(create_page::callback(state, metrics, false))
    }
}

#[compose]
pub(super) fn room_pages(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        PageView (
            id: "nearby_room_pages",
            grow: 1.0,
            min_height: 0,
            page: ${ state.page_key() },
            transition: ${ state.transition() }
        ) on CreatePage(create_page::callback(state, metrics, true))
    }
}
