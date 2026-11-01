use super::consoles::Platform::Favorites;
use super::consoles::{Platform, console};
use super::input::contains;
use super::launcher::{Game, Launcher, PLATFORMS, Screen};
use super::model::DisplayState;
use super::pixel::Scenery;
use super::shelf;
use super::style::*;
use super::widgets::paragraph;
use crate::prelude::*;
use crate::ui::widgets::{FollowBounds, Text};
use crate::ui::{ComputedRect, Hidden, Parent};
use alloc::vec::Vec;
const CARTRIDGE_EDGE: i32 = 2;
const CARTRIDGE_RAIL: i32 = 2;
const CARTRIDGE_CAP: i32 = 4;
const CARTRIDGE_CONTACTS: usize = 8;

#[component]
pub(super) struct LibraryTarget {
    pub key: usize,
    pub shelf: bool,
}
#[component]
pub(super) struct LibraryFocus {
    pub body: Entity,
    pub shelf: bool,
}

#[system(order = Animation)]
pub(super) fn follow(
    library: model!(Launcher),
    frames: query!(Entity, LibraryFocus, optional FollowBounds, optional Hidden),
    targets: query!(Entity, LibraryTarget, ComputedRect),
    parents: query!(Parent),
    commands: commands!(),
) {
    let mut active_frame = None;
    for (frame, marker, bounds, hidden) in frames {
        let active = (library.screen() == Screen::Favorites) == marker.shelf;
        if active {
            active_frame = Some((
                frame,
                marker,
                bounds.map(|old| old.target),
                hidden.is_some(),
            ));
        } else if hidden.is_none() {
            commands.insert(frame, Hidden);
        }
    }
    if let Some((frame, marker, previous, hidden)) = active_frame {
        let selected = if marker.shelf {
            library.shelf_selection()
        } else {
            library.selected()
        };
        let target = targets.into_iter().find_map(|(entity, row, _)| {
            (row.shelf == marker.shelf
                && row.key == selected
                && contains(marker.body, entity, |e| parents.get(e).map(|p| p.0)))
            .then_some(entity)
        });
        if let Some(target) = target {
            if previous != Some(target) {
                commands.insert(
                    frame,
                    FollowBounds::new(target).with_transition(crate::anim::SNAPPY),
                );
            }
            if hidden {
                commands.remove::<Hidden>(frame);
            }
        } else if !hidden {
            commands.insert(frame, Hidden);
        }
    }
}

#[compose]
pub(super) fn focus(body: Entity, library: model!(Launcher), shelf: bool) -> Entity {
    ui! {
        View (position: Position::Absolute, width: 0, height: 0) [
            LibraryFocus { body, shelf },
            crate::ui::IgnoreHitTest,
        ] {
            View (
                position: Position::Absolute,
                left: ${ Dimension::Percent(-library.card_motion().expansion().0) },
                top: ${ Dimension::Percent(-library.card_motion().expansion().1) },
                width: ${ Dimension::Percent(Fixed::from_int(100) + library.card_motion().expansion().0 * 2) },
                height: ${ Dimension::Percent(Fixed::from_int(100) + library.card_motion().expansion().1 * 2) },
                border_width: 2,
                border_color: ACCENT
            ) [crate::ui::IgnoreHitTest]
        }
    }
}

#[compose]
pub(super) fn favorites(state: model!(DisplayState), library: model!(Launcher)) -> Entity {
    ui! { favorites_grid (state, library, library.shelf()) }
}

#[compose]
fn favorites_grid(
    state: model!(DisplayState),
    library: model!(Launcher),
    grid: model!(shelf::Shelf),
) -> Entity {
    let metrics = state.metrics();
    let body = ui! {
        Column (
            id: "nearby_shelf",
            grow: 1.0, min_height: 0, min_width: 0, bg_color: PANEL,
            padding: Padding::all(shelf::INSET), row_gap: 4
        ) [Scenery::Ridge] {
            if ${ library.games().iter().all(|game| !game.favorite()) } {
                console (Favorites)
            } else {
                Column (grow: 1.0, min_height: 0, min_width: 0, row_gap: shelf::GAP, align: AlignItems::Center, justify: JustifyContent::Center) {
                    walk ${ 0..grid.plan().grid.rows } with line {
                        Row (width: ${ grid.plan().width() }, height: ${ grid.plan().card_height }, shrink: 0.0, min_height: 0, column_gap: shelf::GAP) {
                            walk ${
                                library.games().entries().filter(|(_, game)| game.favorite())
                                    .skip(grid.page() * grid.plan().grid.columns * grid.plan().grid.rows + line * grid.plan().grid.columns)
                                    .take(grid.plan().grid.columns).collect::<Vec<_>>()
                            } with item by item.0 {
                                cartridge (item.1, library, metrics)
                            }
                        }
                    }
                }
            }
            Row (height: 4, shrink: 0.0, column_gap: 4, justify: JustifyContent::Center) {
                walk ${
                    let (first, end, selected) = grid.pages();
                    (first..end).map(|i| (i, i == selected)).collect::<Vec<_>>()
                } with page by page.0 {
                    View (width: if page.1 { 12 } else { 4 }, bg_color: if page.1 { ACCENT } else { DIVIDER })
                }
            }
        } on SizeChanged { grid.set_size(event.new.width, event.new.height); }
    };
    ui! { focus (body, library, true) }
}

#[compose]
fn cartridge(game: model!(Game), library: model!(Launcher), metrics: Metrics) -> Entity {
    ui! { cartridge_grid (game, library, library.shelf(), metrics) }
}

#[compose]
fn cartridge_grid(
    game: model!(Game),
    library: model!(Launcher),
    grid: model!(shelf::Shelf),
    metrics: Metrics,
) -> Entity {
    let key = game.key() as usize;
    ui! {
        Column (
            width: ${ grid.plan().card_width }, shrink: 0.0, min_width: 0, min_height: 0,
            transform: ${ library.card_motion().card_transform(key) }
        ) {
            Column (
                grow: 1.0, min_width: 0, min_height: 0, padding: Padding::all(shelf::CARD_INSET),
                container: true, bg_color: ${ library.card_motion().card_color(key) }
            ) [LibraryTarget { key, shelf: true }] {
                cartridge_source (game, metrics)
                Row (
                    height: shelf::LABEL_HEIGHT, shrink: 0.0, min_width: 0, container: true,
                    padding: Padding { left: metrics.gap.into(), right: shelf::LABEL_ACTOR_SPACE.into(), ..Default::default() }
                ) {
                    Text (
                        text: ${ game.name() }, grow: 1.0, min_width: 0, font: FONT, font_size: 12,
                        text_color: FOREGROUND, paragraph: paragraph(crate::ui::widgets::TextAlign::Start, 1)
                    ) [FONT_PAINT]
                }
            } on HoverChanged { let hovered = event.hovered; if hovered { library.select(key); } } on Tap { library.choose_game(key as u32, key); }
            View (height: shelf::SHADOW, shrink: 0.0, bg_color: DIVIDER)
        }
    }
}

#[compose]
fn cartridge_source(game: model!(Game), _metrics: Metrics) -> Entity {
    let platform = Platform::from_id(PLATFORMS[game.platform()].1);
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0, row_gap: 0) {
            Column (
                grow: 1.0,
                min_height: 0,
                min_width: 0,
                bg_color: DIVIDER,
                padding: Padding {
                    left: CARTRIDGE_RAIL.into(),
                    right: CARTRIDGE_RAIL.into(),
                    ..Default::default()
                },
                border_width: CARTRIDGE_EDGE,
                border_color: DEEP
            ) {
                Row (
                    height: CARTRIDGE_CAP,
                    shrink: 0.0,
                    align: AlignItems::Center,
                    column_gap: CARTRIDGE_RAIL
                ) {
                    cartridge_rivet ()
                    Column (grow: 1.0, row_gap: 2) {
                        View (height: 1, bg_color: PANEL)
                        View (height: 1, bg_color: SECONDARY)
                    }
                    cartridge_rivet ()
                }
                Row (grow: 1.0, min_height: 0, min_width: 0, column_gap: CARTRIDGE_RAIL * 2) {
                    cartridge_grip ()
                    Column (
                        grow: 1.0,
                        min_height: 0,
                        min_width: 0,
                        bg_color: DEEP,
                        padding: Padding::all(CARTRIDGE_EDGE)
                    ) {
                        console (platform)
                    }
                    cartridge_grip ()
                }
                Row (
                    height: CARTRIDGE_CAP,
                    shrink: 0.0,
                    justify: JustifyContent::Center,
                    align: AlignItems::FlexEnd
                ) {
                    Row (
                        width: Dimension::percent(64),
                        height: 4,
                        padding: Padding {
                            left: CARTRIDGE_RAIL.into(),
                            right: CARTRIDGE_RAIL.into(),
                            ..Default::default()
                        },
                        column_gap: CARTRIDGE_EDGE,
                        bg_color: DEEP
                    ) {
                        walk 0..CARTRIDGE_CONTACTS with _index {
                            Column (grow: 1.0, min_width: 0, bg_color: WARNING) {
                                View (height: CARTRIDGE_EDGE, bg_color: CLAY)
                            }
                        }
                    }
                }
            }
        }
    }
}

#[compose]
fn cartridge_grip() -> Entity {
    ui! {
        Column (
            width: CARTRIDGE_RAIL,
            shrink: 0.0,
            justify: JustifyContent::Center,
            row_gap: CARTRIDGE_RAIL
        ) {
            walk 0..3 with _index {
                Column (height: 3, shrink: 0.0, bg_color: SECONDARY) {
                    View (height: 1, bg_color: PANEL)
                }
            }
        }
    }
}

#[compose]
fn cartridge_rivet() -> Entity {
    ui! {
        Column (
            width: 3,
            height: 3,
            shrink: 0.0,
            bg_color: SECONDARY,
            align: AlignItems::Center,
            justify: JustifyContent::Center
        ) {
            View (width: 3, height: 1, bg_color: PANEL)
        }
    }
}
