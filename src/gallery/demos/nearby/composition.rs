use super::{
    model::{DisplayState, PageKey},
    pages,
    style::*,
};
use crate::prelude::*;
use crate::ui::widgets::navigation::PageTransition;
use crate::ui::widgets::{
    Button, Text,
    page_view::{CreatePage, PageView},
};

#[event]
fn create_page(
    event: event!(CreatePage<PageKey>),
    state: model!(DisplayState),
    metrics: Metrics,
    frame: Entity,
) -> Entity {
    let mut snapshot = state.snapshot();
    snapshot.page = event.route().page;
    ui! { pages::page (&snapshot, state, metrics, frame) }
}

#[compose]
pub(super) fn build_widgets(state: model!(DisplayState), height: u16) {
    let metrics = Metrics::for_height(height);
    ui! {
        Column (grow: 1.0, bg_color: BACKGROUND) {
            Row (height: 36, shrink: 0.0, padding: Padding::all(4), column_gap: 8, align: AlignItems::Center) {
                Text ("LOCAL DEMO", grow: 1.0, font: FONT, font_size: 12, text_color: SECONDARY)
                Button (id: "nearby_back", width: 58, height: 28, border_radius: 0, normal_color: PANEL, text_color: FOREGROUND, font: FONT, font_size: 12) [Text::label("Back")]
                    on Tap { state.back_page(); }
                Button (id: "nearby_advance", width: 86, height: 28, border_radius: 0, normal_color: ACCENT, text_color: BACKGROUND, font: FONT, font_size: 12) [Text::label("Next step")]
                    on Tap { state.advance(); }
                Button (id: "nearby_refresh", width: 58, height: 28, border_radius: 0, normal_color: PANEL, text_color: FOREGROUND, font: FONT, font_size: 12) [Text::label("Refresh")]
                    on Tap { state.refresh_rooms(); }
                Button (id: "nearby_restart", width: 58, height: 28, border_radius: 0, normal_color: PANEL, text_color: FOREGROUND, font: FONT, font_size: 12) [Text::label("Restart")]
                    on Tap { state.restart(); }
            }
            page_host (state, metrics, id("nearby_focus"))
            View (id: "nearby_focus", border_color: ACCENT, border_width: 2) [
                crate::ui::widgets::FollowBounds::new(id("nearby_pages"))
                    .with_transition(crate::anim::TweenConfig::new(220, &crate::anim::ease::IN_OUT_CUBIC)),
                crate::ui::IgnoreHitTest,
            ]
        }
    };
}

#[compose]
fn page_host(state: model!(DisplayState), metrics: Metrics, frame: Entity) -> Entity {
    ui! {
        PageView (id: "nearby_pages", page: ${ state.page_key() }, grow: 1.0, min_height: 0,
            bg_color: BACKGROUND, clip_children: true,
            transition: PageTransition::None
        ) on CreatePage(create_page::callback(state, metrics, frame))
    }
}
