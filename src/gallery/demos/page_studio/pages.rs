use super::model::{Route, Scene};
use super::style::*;
use crate::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::widgets::{Button, Placeholder, Slider, Switch, Text, TextInput};
use crate::ui::widgets::{
    navigation::{CreatePage, NavigationPath},
    page_view,
};

pub(super) const PLACES: [&str; 6] = [
    "Quiet garden",
    "The blue coast",
    "After midnight",
    "Moss & morning",
    "Open water",
    "Violet hour",
];

#[compose]
pub(super) fn artwork(index: u8, height: i32) -> Entity {
    let (sky, hill) = colors(index);
    ui! {
        View (height: height, bg_color: sky, clip_children: true, border_radius: 16) {
            View (
                position: Position::Absolute,
                left: Dimension::percent(60),
                top: 22,
                width: 100,
                height: 100,
                border_radius: 50,
                bg_color: CARD
            )
            View (
                position: Position::Absolute,
                left: -30,
                top: height / 2,
                width: 340,
                height: 200,
                border_radius: 100,
                bg_color: hill
            )
            View (
                position: Position::Absolute,
                left: Dimension::percent(50),
                top: height * 2 / 3,
                width: 250,
                height: 150,
                border_radius: 75,
                bg_color: INK
            )
            Text (
                "FIELD / NOTES",
                position: Position::Absolute,
                left: 18,
                top: 16,
                width: 180,
                height: 22,
                font_size: 11,
                text_color: INK
            )
        }
    }
}

#[compose]
pub(super) fn home(path: model!(NavigationPath<Route>)) -> Entity {
    ui! {
        Column (grow: 1.0, padding: Padding::all(22), row_gap: 12) {
            artwork (0, 184)
            Text ("Somewhere slower.", height: 38, font_size: 27, text_color: INK)
            Text (
                "A small collection of places to get lost in.",
                height: 24,
                font_size: 13,
                text_color: MUTED
            )
            View (grow: 1.0)
            Button (
                id: "pages_explore",
                height: 44,
                border_radius: 12,
                normal_color: INK,
                pressed_color: MUTED,
                text_color: CARD,
                font_size: 14
            ) [
                Text::label("Explore the collection  >"),
            ] on Tap {
                if let Err(error) = path.push(Route::Library) {
                    crate::warn!("navigation rejected: {:?}", error);
                }
            }
        }
    }
}

#[event]
pub(super) fn create_page(
    event: event!(CreatePage<Route>),
    path: model!(NavigationPath<Route>),
) -> Entity {
    match *event.route() {
        Route::Library => ui! {
            library (path)
        },
        Route::Place(index) => ui! {
            place (path, index)
        },
        Route::Notes => ui! {
            notes ()
        },
    }
}

#[compose]
fn library(path: model!(NavigationPath<Route>)) -> Entity {
    ui! {
        Column (grow: 1.0, padding: Padding::all(22), row_gap: 10) {
            Text ("The collection", height: 38, font_size: 26, text_color: INK)
            Text (
                "Scroll, open a place, then come back.",
                height: 24,
                font_size: 13,
                text_color: MUTED
            )
            Column (id: "pages_collection", grow: 1.0, row_gap: 10, clip_children: true) [
                ScrollOffset::default(),
                ScrollConfig {
                    direction: ScrollAxis::Vertical,
                    elastic: true,
                    ..Default::default()
                },
            ] {
                walk PLACES.iter().enumerate() with entry {
                    place_card (path.clone(), entry.0 as u8)
                }
            }
        }
    }
}

#[compose]
fn place_card(path: model!(NavigationPath<Route>), index: u8) -> Entity {
    let (bg, accent) = colors(index);
    ui! {
        Row (
            height: 90,
            padding: Padding::all(14),
            column_gap: 16,
            align: AlignItems::Center,
            border_radius: 14,
            bg_color: bg
        ) {
            Text (
                text: alloc::format!("{:02}", index + 1),
                width: 56,
                height: 48,
                font_size: 30,
                text_color: accent
            )
            Column (grow: 1.0, row_gap: 6) {
                Text (PLACES[index as usize], height: 26, font_size: 18, text_color: INK)
                Text ("OPEN JOURNAL   >", height: 18, font_size: 10, text_color: INK)
            }
        } on Tap {
            if let Err(error) = path.push(Route::Place(index)) {
                crate::warn!("navigation rejected: {:?}", error);
            }
        }
    }
}

#[compose]
fn place(path: model!(NavigationPath<Route>), index: u8) -> Entity {
    ui! {
        Column (grow: 1.0, padding: Padding::all(22), row_gap: 12) {
            artwork (index, 152)
            Text (PLACES[index as usize], height: 34, font_size: 26, text_color: INK)
            Row (height: 28, align: AlignItems::Center) {
                Text ("Save for later", grow: 1.0, height: 26, font_size: 14, text_color: MUTED)
                Switch (width: 44, height: 24)
            }
            Slider (
                height: 26,
                value: Fixed::from_ratio(2, 5),
                fill_color: INK,
                track_color: LINE,
                thumb_color: INK
            )
            View (grow: 1.0)
            Button (
                height: 42,
                border_radius: 12,
                normal_color: MINT,
                pressed_color: LINE,
                text_color: INK,
                font_size: 14
            ) [
                Text::label("Write a note  >"),
            ] on Tap {
                if let Err(error) = path.push(Route::Notes) {
                    crate::warn!("navigation rejected: {:?}", error);
                }
            }
        }
    }
}

#[compose]
fn notes() -> Entity {
    ui! {
        Column (grow: 1.0, padding: Padding::all(24), row_gap: 16) {
            Text ("A note to keep.", height: 44, font_size: 28, text_color: INK)
            Text ("Take your time.", height: 28, font_size: 15, text_color: MUTED)
            TextInput (
                height: 52,
                capacity: 128,
                font_size: 17,
                border_radius: 12,
                bg_color: PAPER,
                text_color: INK,
                border_color: LINE,
                border_width: 1
            ) [
                Placeholder("What caught your eye?"),
            ]
            View (grow: 1.0)
            Text (
                "HOME / COLLECTION / PLACE / NOTE",
                height: 24,
                font_size: 10,
                text_color: MUTED
            )
        }
    }
}

#[event]
pub(super) fn create_scene(event: event!(page_view::CreatePage<Scene>)) -> Entity {
    let (index, title, subtitle) = match event.route() {
        Scene::Garden => (0, "Breathe in.", "GARDEN / 06:24"),
        Scene::Coast => (1, "Drift away.", "COAST / 14:08"),
        Scene::Night => (2, "Stay a little.", "NIGHT / 23:16"),
    };
    ui! {
        Column (grow: 1.0, padding: Padding::all(20), row_gap: 14) {
            artwork (index, 214)
            Text (subtitle, height: 20, font_size: 11, text_color: MUTED)
            Text (title, height: 44, font_size: 28, text_color: INK)
            View (grow: 1.0)
            Text ("A MOMENT, NOT A DESTINATION.", height: 22, font_size: 10, text_color: MUTED)
        }
    }
}
