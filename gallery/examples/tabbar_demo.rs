extern crate alloc;

use mirui::app::App;
use mirui::components::tab_pages::TabContent;
use mirui::components::tabbar::TabBar;
use mirui::layout::*;
use mirui::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension};
use mirui::widget::builder::WidgetBuilder;
use mirui_macros::ui;

fn main() {
    let backend = SdlSurface::new("TabBar Demo", 480, 320);
    let mut app = App::new(backend);

    app.add_system(mirui::anim::sync_delta_time_ms);

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            ..Default::default()
        })
        .id();

    let tabs = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        tabbar (
            bg_color: Color::rgb(40, 40, 56),
            width: 480,
            height: 40
        ) [
            TabBar::new(3).with_indicator(Color::rgb(88, 166, 255), 3),
        ] {
            tab0 (
                text: "Home",
                text_color: Color::rgb(220, 220, 230),
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {}
            tab1 (
                text: "Search",
                text_color: Color::rgb(220, 220, 230),
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {}
            tab2 (
                text: "Profile",
                text_color: Color::rgb(220, 220, 230),
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {}
        }
    };

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        content_root (
            bg_color: Color::rgb(20, 20, 30),
            width: 480,
            height: 280
        ) {
            home_page (
                bg_color: Color::rgb(63, 185, 80),
                text: "Home page",
                text_color: Color::rgb(255, 255, 255),
                width: 480,
                height: 280,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) [
                TabContent {
                    tab_bar: tabs,
                    index: 0,
                },
            ] {}
            search_page (
                bg_color: Color::rgb(255, 165, 80),
                text: "Search page",
                text_color: Color::rgb(255, 255, 255),
                width: 480,
                height: 280,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) [
                TabContent {
                    tab_bar: tabs,
                    index: 1,
                },
            ] {}
            profile_page (
                bg_color: Color::rgb(210, 168, 255),
                text: "Profile page",
                text_color: Color::rgb(40, 40, 56),
                width: 480,
                height: 280,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) [
                TabContent {
                    tab_bar: tabs,
                    index: 2,
                },
            ] {}
        }
    };

    app.set_root(root);
    app.add_plugin(StdInstantClockPlugin::default())
        .add_plugin(FpsSummaryPlugin::default());
    app.run();
}
