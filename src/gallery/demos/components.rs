extern crate alloc;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::assets::*;
use crate::components::{Button, Checkbox, Image, ProgressBar};
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::widget::{Children, Parent, Style};

pub fn build_widgets(world: &mut World, parent: Entity) {
    if let Some(style) = world.get_mut::<Style>(parent) {
        style.bg_color = Some(Color::rgb(24, 24, 37).into());
        style.layout = LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            padding: Padding::all(8),
            grow: Fixed::ONE,
            ..Default::default()
        };
    }

    let _header_root = ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: Color::rgb(30, 102, 245),
            height: 40,
            border_radius: 8,
            direction: FlexDirection::Row,
            align: AlignItems::Center,
            padding: Padding::all(8)
        ) {
            View (text: "mirui Components", grow: 1.0) {}
            View (bg_color: Color::rgb(255, 200, 50), width: 16, height: 16, border_radius: 8) {}
        }
    };

    let badge_img = WidgetBuilder::new(world)
        .layout(LayoutStyle {
            width: Dimension::Px(Fixed::from_int(IMG_THUMBS_UP.width as i32)),
            height: Dimension::Px(Fixed::from_int(IMG_THUMBS_UP.height as i32)),
            ..Default::default()
        })
        .id();
    world.insert(badge_img, Image::new(&IMG_THUMBS_UP));
    world.insert(badge_img, Parent(parent));
    if let Some(children) = world.get_mut::<Children>(parent) {
        children.0.push(badge_img);
    }

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
        Button::new()
            .with_normal_color(Color::rgb(63, 185, 80))
            .with_pressed_color(Color::rgb(40, 140, 55)),
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
        Button::new()
            .with_normal_color(Color::rgb(248, 81, 73))
            .with_pressed_color(Color::rgb(200, 50, 45)),
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
    world.insert(btn_row, Parent(parent));
    if let Some(children) = world.get_mut::<Children>(parent) {
        children.0.push(btn_row);
    }

    let pb1 = WidgetBuilder::new(world)
        .border_radius(4)
        .layout(LayoutStyle {
            height: Dimension::px(12),
            ..Default::default()
        })
        .id();
    world.insert(pb1, ProgressBar::new());
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
        ProgressBar::new().with_fill_color(Color::rgb(63, 185, 80)),
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
        ProgressBar::new().with_fill_color(Color::rgb(248, 81, 73)),
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
    world.insert(pb_col, Parent(parent));
    if let Some(children) = world.get_mut::<Children>(parent) {
        children.0.push(pb_col);
    }

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
        Checkbox::new()
            .with_checked_color(Color::rgb(88, 166, 255))
            .with_unchecked_color(Color::rgb(80, 80, 100)),
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
        Checkbox::new()
            .with_checked_color(Color::rgb(63, 185, 80))
            .with_unchecked_color(Color::rgb(80, 80, 100)),
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
        Checkbox::new()
            .with_checked_color(Color::rgb(248, 81, 73))
            .with_unchecked_color(Color::rgb(80, 80, 100)),
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
    world.insert(cb_row, Parent(parent));
    if let Some(children) = world.get_mut::<Children>(parent) {
        children.0.push(cb_row);
    }

    ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: Color::rgb(40, 40, 55),
            height: 30,
            border_radius: 6,
            text: "Button | ProgressBar | Checkbox | Image"
        ) {}
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::IdMap;
    use crate::widget::builder::WidgetBuilder;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
