use super::{
    actor::actor,
    launcher_view::launcher,
    model::{DisplayState, PageKey},
    pages::page,
    simulation::{InspectorHost, Visibility, controls},
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
pub(super) fn build_widgets(
    state: model!(DisplayState),
    native_size: Option<crate::surface::SurfaceSizeControl>,
) {
    let host = native_size
        .or_else(|| {
            res!(optional crate::surface::SurfaceSizeControl)
                .with(|control| control.map(|control| (*control).clone()))
        })
        .map(InspectorHost::new);
    let size_effect = host.clone();
    state
        .on_effect(move |visible: &Visibility| {
            if let Some(host) = &size_effect {
                host.request(visible.0);
            } else if visible.0 {
                crate::warn!("this surface cannot expand the simulation panel");
            }
        })
        .expect("Nearby inspector host is registered once");
    let width_host = host.clone();
    let height_host = host.clone();
    ui! {
        Row (grow: 1.0, min_height: 0, bg_color: BACKGROUND, clip_children: true) {
            if ${ host.as_ref().is_some_and(InspectorHost::visible) } {
                controls (state)
            }
            Column (
                id: "nearby_demo_content",
                width: ${
                    width_host
                        .as_ref()
                        .map_or(
                            Dimension::percent(100),
                            |host| Dimension::px(i32::from(host.main_size().0)),
                        )
                },
                height: ${
                    height_host
                        .as_ref()
                        .map_or(
                            Dimension::percent(100),
                            |host| Dimension::px(i32::from(host.main_size().1)),
                        )
                },
                shrink: 0.0,
                min_height: 0,
                clip_children: true,
                bg_color: BACKGROUND
            ) {
                if ${ state.launcher().active() || state.return_to_launcher() } {
                    launcher (state.clone(), state.launcher())
                } else {
                    page_host (state)
                    actor (state, false)
                }
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
