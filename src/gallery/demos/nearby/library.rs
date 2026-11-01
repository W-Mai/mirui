use super::consoles::Platform::Favorites as EmptyFavorites;
use super::consoles::{Platform, console};
use super::input::contains;
use super::launcher::Screen::Favorites;
use super::launcher::{Game, Launcher, PLATFORMS, Screen};
use super::model::DisplayState;
use super::pixel::Scenery;
use super::shelf;
use super::style::*;
use super::widgets::paragraph;
use crate::prelude::*;
use crate::ui::widgets::Image;
use crate::ui::widgets::{FollowBounds, Text};
use crate::ui::{Hidden, Parent};
use alloc::vec::Vec;
const CARTRIDGE_EDGE: i32 = 2;
const CARTRIDGE_RAIL: i32 = 2;
const CARTRIDGE_CAP: i32 = 4;
const CARTRIDGE_CONTACTS: usize = 8;

#[compose]
pub(super) fn library_icon(metrics: Metrics) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            min_width: 0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(metrics.gap),
            bg_color: SKY
        ) {
            View (grow: 1.0, min_height: 0, width: Dimension::percent(100)) {
                Image (
                    src: path!(M 22 20 H 138 V 111 H 136 V 116 H 128 V 111 H 32 V 116 H 24 V 111 H 22 Z),
                    viewbox: 160,
                    position: Position::Absolute,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    color: FOREGROUND
                )
                Image (
                    src: path!(M 27 25 H 133 V 106 H 27 Z),
                    viewbox: 160,
                    position: Position::Absolute,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    color: PANEL
                )
                Image (
                    src: path!(
                        M 31 61 H 129 V 66 H 31 Z M 31 96 H 129 V 101 H 31 Z M 87 34 H 118 V 57 H 87 Z M
                        35 73 H 63 V 92 H 35 Z
                    ),
                    viewbox: 160,
                    position: Position::Absolute,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    color: ACCENT
                )
                Image (
                    src: path!(M 35 32 H 56 V 57 H 35 Z M 69 71 H 89 V 92 H 69 Z),
                    viewbox: 160,
                    position: Position::Absolute,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    color: CLAY
                )
                Image (
                    src: path!(M 61 31 H 82 V 57 H 61 Z M 95 71 H 118 V 92 H 95 Z),
                    viewbox: 160,
                    position: Position::Absolute,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    color: WARNING
                )
            }
            Image (
                src: path!(M 0 0 L 9 8 L 18 0 V 5 L 9 14 L 0 5 Z),
                viewbox: 18,
                width: 18,
                height: 14,
                shrink: 0.0,
                color: ACCENT
            )
        }
    }
}

#[component]
pub(super) struct LibraryTarget {
    pub key: usize,
    pub screen: Screen,
}
#[component]
pub(super) struct LibraryFocus {
    pub body: Entity,
    pub screen: Screen,
    pub library: model!(Launcher),
}

#[event]
fn initialize_focus(
    node: entity!(),
    frames: query!(LibraryFocus),
    targets: query!(Entity, LibraryTarget),
    parents: query!(Parent),
    commands: commands!(),
) {
    let Some(frame) = frames.get(node) else {
        return;
    };
    let library = &frame.library;
    if !library.active() || library.menu() || library.screen() != frame.screen {
        return;
    }
    let selected = if frame.screen == Screen::Favorites {
        library.shelf_selection()
    } else {
        library.selected()
    };
    if let Some(target) = targets.into_iter().find_map(|(entity, row)| {
        (row.screen == frame.screen
            && row.key == selected
            && contains(frame.body, entity, |e| parents.get(e).map(|p| p.0)))
        .then_some(entity)
    }) {
        commands.insert(
            node,
            FollowBounds::new(target).with_transition(crate::anim::SNAPPY),
        );
    }
}

#[view(component = LibraryFocus, attach = initialize_focus::callback())]
pub(super) fn library_focus() {}

#[system(order = Animation)]
pub(super) fn follow(
    library: model!(Launcher),
    frames: query!(Entity, LibraryFocus, optional FollowBounds, optional Hidden),
    targets: query!(Entity, LibraryTarget),
    parents: query!(Parent),
    commands: commands!(),
) {
    let mut active_frame = None;
    for (frame, marker, bounds, hidden) in frames {
        let active = library.active()
            && !library.menu()
            && library.screen() == marker.screen
            && !(library.screen() == Screen::Search
                && !library.keyboard()
                && library.search_empty());
        if active {
            active_frame = Some((
                frame,
                marker,
                bounds.map(|old| old.target),
                hidden.is_some(),
            ));
        } else {
            if hidden.is_none() {
                commands.insert(frame, Hidden);
            }
            if bounds.is_some() {
                commands.remove::<FollowBounds>(frame);
            }
        }
    }
    if let Some((frame, marker, previous, hidden)) = active_frame {
        let selected = if marker.screen == Screen::Favorites {
            library.shelf_selection()
        } else {
            library.selected()
        };
        let target = targets.into_iter().find_map(|(entity, row)| {
            (row.screen == marker.screen
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
        } else {
            if !hidden {
                commands.insert(frame, Hidden);
            }
            if previous.is_some() {
                commands.remove::<FollowBounds>(frame);
            }
        }
    }
}

#[compose]
pub(super) fn focus(body: Entity, library: model!(Launcher), screen: Screen) -> Entity {
    ui! {
        View (position: Position::Absolute, width: 0, height: 0) [
            LibraryFocus { body, screen, library: library.clone() },
            crate::ui::IgnoreHitTest,
        ] {
            View (
                position: Position::Absolute,
                left: ${ Dimension::Percent(-if screen == Screen::Favorites { library.shelf_card_motion() } else { library.card_motion() }.expansion().0) },
                top: ${ Dimension::Percent(-if screen == Screen::Favorites { library.shelf_card_motion() } else { library.card_motion() }.expansion().1) },
                width: ${ Dimension::Percent(Fixed::from_int(100) + if screen == Screen::Favorites { library.shelf_card_motion() } else { library.card_motion() }.expansion().0 * 2) },
                height: ${ Dimension::Percent(Fixed::from_int(100) + if screen == Screen::Favorites { library.shelf_card_motion() } else { library.card_motion() }.expansion().1 * 2) },
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
                console (EmptyFavorites)
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
    ui! { focus (body, library, Favorites) }
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
            transform: ${ library.shelf_card_motion().card_transform(key) }
        ) {
            Column (
                grow: 1.0, min_width: 0, min_height: 0, padding: Padding::all(shelf::CARD_INSET),
                container: true, bg_color: ${ library.shelf_card_motion().card_color(key) }
            ) [LibraryTarget { key, screen: Screen::Favorites }] {
                if game.directory() { library_icon (metrics) } else { cartridge_source (game, metrics) }
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
