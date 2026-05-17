use mirui::app::App;
use mirui::components::assets::*;
use mirui::components::button::Button;
use mirui::components::checkbox::Checkbox;
use mirui::components::image::Image;
use mirui::components::progress_bar::ProgressBar;
use mirui::ecs::{Entity, World};
use mirui::layout::*;
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed};
use mirui::widget::builder::WidgetBuilder;
use mirui_macros::ui;

extern crate alloc;

fn build_ui(world: &mut World) -> Entity {
    let root = WidgetBuilder::new(world)
        .bg_color(Color::rgb(24, 24, 37))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            padding: Padding::all(8),
            ..Default::default()
        })
        .id();

    // Header with image
    let _header_root = ui! {
        :(
            parent: root
            world: world
        :)

        header (
            bg_color: Color::rgb(30, 102, 245),
            height: 40,
            border_radius: 8,
            direction: FlexDirection::Row,
            align: AlignItems::Center,
            padding: Padding::all(8)
        ) {
            title (text: "mirui Components", grow: 1.0) {}
            badge (bg_color: Color::rgb(255, 200, 50), width: 16, height: 16, border_radius: 8) {}
        }
    };

    // Attach thumbs-up image to badge
    let badge_img = WidgetBuilder::new(world)
        .layout(LayoutStyle {
            width: Dimension::Px(Fixed::from_int(IMG_THUMBS_UP.width as i32)),
            height: Dimension::Px(Fixed::from_int(IMG_THUMBS_UP.height as i32)),
            ..Default::default()
        })
        .id();
    world.insert(badge_img, Image::new(&IMG_THUMBS_UP));
    // Add to root as standalone element
    use mirui::widget::{Children, Parent};
    world.insert(badge_img, Parent(root));
    if let Some(children) = world.get_mut::<Children>(root) {
        children.0.push(badge_img);
    }

    // Button row
    let btn_ok = WidgetBuilder::new(world)
        .text("OK")
        .border_radius(6)
        .layout(LayoutStyle {
            grow: Fixed::from_f32(1.0),
            height: Dimension::px(36),
            ..Default::default()
        })
        .id();
    world.insert(
        btn_ok,
        Button::new(Color::rgb(63, 185, 80), Color::rgb(40, 140, 55)),
    );

    let btn_cancel = WidgetBuilder::new(world)
        .text("Cancel")
        .border_radius(6)
        .layout(LayoutStyle {
            grow: Fixed::from_f32(1.0),
            height: Dimension::px(36),
            ..Default::default()
        })
        .id();
    world.insert(
        btn_cancel,
        Button::new(Color::rgb(248, 81, 73), Color::rgb(200, 50, 45)),
    );

    let btn_row = WidgetBuilder::new(world)
        .layout(LayoutStyle {
            direction: FlexDirection::Row,
            height: Dimension::px(36),
            ..Default::default()
        })
        .child(btn_ok)
        .child(btn_cancel)
        .id();
    world.insert(btn_row, Parent(root));
    if let Some(children) = world.get_mut::<Children>(root) {
        children.0.push(btn_row);
    }

    // Progress bars
    let pb1 = WidgetBuilder::new(world)
        .border_radius(4)
        .layout(LayoutStyle {
            height: Dimension::px(12),
            ..Default::default()
        })
        .id();
    world.insert(
        pb1,
        ProgressBar::new(Color::rgb(88, 166, 255), Color::rgb(60, 60, 80)),
    );
    if let Some(pb) = world.get_mut::<ProgressBar>(pb1) {
        pb.value = 0.7;
    }

    let pb2 = WidgetBuilder::new(world)
        .border_radius(4)
        .layout(LayoutStyle {
            height: Dimension::px(12),
            ..Default::default()
        })
        .id();
    world.insert(
        pb2,
        ProgressBar::new(Color::rgb(63, 185, 80), Color::rgb(60, 60, 80)),
    );
    if let Some(pb) = world.get_mut::<ProgressBar>(pb2) {
        pb.value = 0.4;
    }

    let pb3 = WidgetBuilder::new(world)
        .border_radius(4)
        .layout(LayoutStyle {
            height: Dimension::px(12),
            ..Default::default()
        })
        .id();
    world.insert(
        pb3,
        ProgressBar::new(Color::rgb(248, 81, 73), Color::rgb(60, 60, 80)),
    );
    if let Some(pb) = world.get_mut::<ProgressBar>(pb3) {
        pb.value = 0.9;
    }

    let pb_col = WidgetBuilder::new(world)
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            height: Dimension::px(50),
            justify: JustifyContent::SpaceBetween,
            ..Default::default()
        })
        .child(pb1)
        .child(pb2)
        .child(pb3)
        .id();
    world.insert(pb_col, Parent(root));
    if let Some(children) = world.get_mut::<Children>(root) {
        children.0.push(pb_col);
    }

    // Checkboxes
    let cb1 = WidgetBuilder::new(world)
        .border_radius(4)
        .layout(LayoutStyle {
            width: Dimension::px(24),
            height: Dimension::px(24),
            ..Default::default()
        })
        .id();
    world.insert(
        cb1,
        Checkbox::new(Color::rgb(88, 166, 255), Color::rgb(80, 80, 100)),
    );
    if let Some(cb) = world.get_mut::<Checkbox>(cb1) {
        cb.checked = true;
    }

    let cb2 = WidgetBuilder::new(world)
        .border_radius(4)
        .layout(LayoutStyle {
            width: Dimension::px(24),
            height: Dimension::px(24),
            ..Default::default()
        })
        .id();
    world.insert(
        cb2,
        Checkbox::new(Color::rgb(63, 185, 80), Color::rgb(80, 80, 100)),
    );

    let cb3 = WidgetBuilder::new(world)
        .border_radius(4)
        .layout(LayoutStyle {
            width: Dimension::px(24),
            height: Dimension::px(24),
            ..Default::default()
        })
        .id();
    world.insert(
        cb3,
        Checkbox::new(Color::rgb(248, 81, 73), Color::rgb(80, 80, 100)),
    );
    if let Some(cb) = world.get_mut::<Checkbox>(cb3) {
        cb.checked = true;
    }

    let cb_row = WidgetBuilder::new(world)
        .layout(LayoutStyle {
            direction: FlexDirection::Row,
            height: Dimension::px(30),
            align: AlignItems::Center,
            ..Default::default()
        })
        .child(cb1)
        .child(cb2)
        .child(cb3)
        .id();
    world.insert(cb_row, Parent(root));
    if let Some(children) = world.get_mut::<Children>(root) {
        children.0.push(cb_row);
    }

    // Footer
    ui! {
        :(
            parent: root
            world: world
        :)

        footer (
            bg_color: Color::rgb(40, 40, 55),
            height: 30,
            border_radius: 6,
            text: "Button | ProgressBar | Checkbox | Image"
        ) {}
    };

    root
}

fn main() {
    let backend = SdlSurface::new("mirui - components demo", 480, 320);
    let mut app = App::new(backend).with_default_widgets();

    let root = build_ui(&mut app.world);
    app.set_root(root);
    app.run();
}
