use super::following::bounds_panel;
use super::model::{Route, Scene, Studio, transition};
use super::pages::{create_page, create_scene, home};
use super::style::*;
use crate::prelude::*;
use crate::ui::widgets::navigation::{NavigationPath, NavigationStack};
use crate::ui::widgets::page_view::PageView;
use crate::ui::widgets::{Button, Text};

#[compose]
pub fn build_widgets(path: model!(NavigationPath<Route>), controls: model!(Studio)) {
    ui! {
        Column (grow: 1.0, padding: Padding::all(24), row_gap: 16, bg_color: PAPER) {
            Row (height: 56, align: AlignItems::Center, column_gap: 18) {
                Text ("P /", width: 64, height: 48, font_size: 34, text_color: INK)
                Column (grow: 1.0) {
                    Text ("Page Studio", height: 34, font_size: 28, text_color: INK)
                    Text (
                        "PLACES, LAYERS & LITTLE DETOURS",
                        height: 18,
                        font_size: 10,
                        text_color: MUTED
                    )
                }
                Text ("MIRUI / 01", width: 112, height: 24, font_size: 12, text_color: MUTED)
            }
            Row (height: 40, column_gap: 8, align: AlignItems::Center) {
                Text ("MOTION", width: 68, height: 24, font_size: 11, text_color: MUTED)
                walk ["None", "Slide", "Fade"].iter().enumerate() with item {
                    effect_button (controls.clone(), item.0 as u8, *item.1)
                }
                View (grow: 1.0)
                Text ("SPRING", width: 68, height: 24, font_size: 11, text_color: MUTED)
                walk ["Smooth", "Snappy", "Bouncy"].iter().enumerate() with item {
                    spring_button (controls.clone(), item.0 as u8, *item.1)
                }
            }
            Row (grow: 1.0, column_gap: 20) {
                Column (width: 524, row_gap: 12) {
                    Row (height: 32, align: AlignItems::Center, column_gap: 8) {
                        Text (
                            "01 / NAVIGATION STACK",
                            grow: 1.0,
                            height: 24,
                            font_size: 11,
                            text_color: MUTED
                        )
                        Button (
                            id: "pages_back",
                            width: 68,
                            height: 30,
                            font_size: 12,
                            border_radius: 8,
                            normal_color: CARD,
                            pressed_color: LINE,
                            text_color: INK
                        ) [
                            Text::label("< Back"),
                        ] on Tap {
                            if let Err(error) = path.pop() {
                                crate::warn!("navigation rejected: {:?}", error);
                            }
                        }
                        Button (
                            id: "pages_home",
                            width: 62,
                            height: 30,
                            font_size: 12,
                            border_radius: 8,
                            normal_color: CARD,
                            pressed_color: LINE,
                            text_color: INK
                        ) [
                            Text::label("Home"),
                        ] on Tap {
                            if let Err(error) = path.clear() {
                                crate::warn!("navigation rejected: {:?}", error);
                            }
                        }
                    }
                    NavigationStack (
                        id: "pages_stack",
                        path: path.clone(),
                        grow: 1.0,
                        bg_color: CARD,
                        border_radius: 20,
                        clip_children: true,
                        transition: ${ transition(controls.effect(), controls.spring()) }
                    ) {
                        home (path)
                    } on CreatePage(create_page::callback(path))
                }
                Column (grow: 1.0, row_gap: 12) {
                    Row (height: 32, align: AlignItems::Center) {
                        Text (
                            "02 / PAGE VIEW",
                            grow: 1.0,
                            height: 24,
                            font_size: 11,
                            text_color: MUTED
                        )
                        Button (
                            id: "pages_cycle",
                            width: 72,
                            height: 30,
                            font_size: 12,
                            border_radius: 8,
                            normal_color: INK,
                            pressed_color: MUTED,
                            text_color: CARD
                        ) [
                            Text::label("Next >"),
                        ] on Tap { controls.next(); }
                    }
                    PageView (
                        id: "pages_scene",
                        page: ${ controls.scene() },
                        grow: 1.0,
                        bg_color: CARD,
                        border_radius: 20,
                        clip_children: true,
                        transition: ${ transition(controls.effect(), controls.spring()) }
                    ) on CreatePage(create_scene::callback())
                    bounds_panel (controls)
                }
            }
            Row (height: 34, align: AlignItems::Center, column_gap: 10) {
                Text (
                    text: ${ format_args!("DEPTH  {:02} / 08", path.len()) },
                    capacity: 16,
                    width: 160,
                    height: 24,
                    font_size: 11,
                    text_color: INK
                )
                Text (
                    "Go back. Your place stays.",
                    grow: 1.0,
                    height: 24,
                    font_size: 12,
                    text_color: MUTED
                )
                walk [(Scene::Garden, "Garden"), (Scene::Coast, "Coast"), (Scene::Night, "Night")].iter() with item {
                    scene_button (controls.clone(), item.0, item.1)
                }
            }
        }
    };
}

#[compose]
fn effect_button(controls: model!(Studio), index: u8, label: &'static str) -> Entity {
    ui! {
        Button (
            width: 72,
            height: 34,
            border_radius: 10,
            font_size: 12,
            normal_color: ${ if controls.effect() == index { INK } else { CARD } },
            text_color: ${ if controls.effect() == index { CARD } else { INK } },
            pressed_color: MUTED
        ) [
            Text::label(label),
        ] on Tap { controls.set_effect(index); }
    }
}

#[compose]
fn spring_button(controls: model!(Studio), index: u8, label: &'static str) -> Entity {
    ui! {
        Button (
            width: 78,
            height: 34,
            border_radius: 10,
            font_size: 12,
            normal_color: ${ if controls.spring() == index { MINT } else { CARD } },
            text_color: INK,
            pressed_color: LINE
        ) [
            Text::label(label),
        ] on Tap { controls.set_spring(index); }
    }
}

#[compose]
fn scene_button(controls: model!(Studio), scene: Scene, label: &'static str) -> Entity {
    ui! {
        Button (
            width: 82,
            height: 30,
            border_radius: 10,
            font_size: 12,
            normal_color: ${ if controls.scene() == scene { INK } else { PAPER } },
            text_color: ${ if controls.scene() == scene { CARD } else { MUTED } },
            pressed_color: LINE
        ) [
            Text::label(label),
        ] on Tap { controls.select(scene); }
    }
}
