use mirui::anim::{Animation, FrameClock, PlayMode, ease};
use mirui::app::App;
use mirui::components::slider::Slider;
use mirui::components::switch::Switch;
use mirui::ecs::{Entity, World};
use mirui::event::GestureHandler;
use mirui::event::gesture::GestureEvent;
use mirui::layout::*;
use mirui::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed};
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::dirty::Dirty;
use mirui_macros::ui;

extern crate alloc;

mirui_macros::animation!(AnimateThumbX, |world, entity, value| {
    mirui::widget::set_position(world, entity, value, Fixed::from_int(3));
});

struct SliderTrackWidth(Fixed);

fn slider_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    match event {
        GestureEvent::DragMove { x, .. } | GestureEvent::Tap { x, .. } => {
            let track_w = world
                .get::<SliderTrackWidth>(entity)
                .map(|t| t.0)
                .unwrap_or(Fixed::from_int(200));
            if let Some(style) = world.get::<mirui::widget::Style>(entity) {
                let left = style.layout.left;
                let local_x = *x
                    - match left {
                        Dimension::Px(p) => p,
                        _ => Fixed::ZERO,
                    };
                let ratio = local_x / track_w;
                if let Some(slider) = world.get_mut::<Slider>(entity) {
                    slider.set_ratio(ratio);
                    let fill_w = slider.ratio() * track_w;
                    let fill_color = slider.fill_color;
                    if let Some(children) = world.get::<mirui::widget::Children>(entity) {
                        let children_copy: alloc::vec::Vec<Entity> = children.0.clone();
                        if children_copy.len() >= 2 {
                            let fill_entity = children_copy[0];
                            let thumb_entity = children_copy[1];
                            if let Some(style) = world.get_mut::<mirui::widget::Style>(fill_entity)
                            {
                                style.layout.width = Dimension::Px(fill_w);
                                style.bg_color = Some(fill_color);
                            }
                            world.insert(fill_entity, Dirty);
                            let thumb_x = fill_w - Fixed::from_int(8);
                            mirui::widget::set_position(
                                world,
                                thumb_entity,
                                thumb_x.max(Fixed::ZERO),
                                Fixed::from_int(0),
                            );
                        }
                    }
                }
            }
            world.insert(entity, Dirty);
            true
        }
        _ => false,
    }
}

fn switch_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    match event {
        GestureEvent::Tap { .. } => {
            let (is_on, track_color) = {
                let Some(sw) = world.get_mut::<Switch>(entity) else {
                    return false;
                };
                sw.toggle();
                (sw.on, sw.track_color())
            };
            if let Some(style) = world.get_mut::<mirui::widget::Style>(entity) {
                style.bg_color = Some(track_color);
            }
            world.insert(entity, Dirty);

            if let Some(children) = world.get::<mirui::widget::Children>(entity) {
                if let Some(&thumb) = children.0.first() {
                    let target_x = if is_on {
                        Fixed::from_int(26)
                    } else {
                        Fixed::from_int(3)
                    };
                    let current_x = world
                        .get::<mirui::widget::Style>(thumb)
                        .and_then(|s| match s.layout.left {
                            Dimension::Px(p) => Some(p),
                            _ => None,
                        })
                        .unwrap_or(Fixed::from_int(3));
                    world.insert(
                        thumb,
                        AnimateThumbX(Animation::new(
                            current_x,
                            target_x,
                            200,
                            ease::ease_out_cubic,
                            PlayMode::Once,
                        )),
                    );
                }
            }
            true
        }
        _ => false,
    }
}

fn main() {
    let backend = SdlSurface::new("mirui - slider & switch", 320, 200);
    let mut app = App::new(backend);

    use std::sync::OnceLock;
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START.get_or_init(std::time::Instant::now);
    app.world.insert_resource(FrameClock::new(|| {
        START.get().unwrap().elapsed().as_nanos() as u64
    }));

    app.add_system(mirui::anim::sync_delta_time_ms);
    app.add_system(AnimateThumbX::system());

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(320),
            height: Dimension::px(200),
            padding: Padding {
                top: Dimension::px(30),
                left: Dimension::px(20),
                right: Dimension::px(20),
                bottom: Dimension::px(30),
            },
            ..Default::default()
        })
        .id();

    // --- Slider ---
    let slider_track = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        slider_track (
            bg_color: Color::rgb(60, 60, 80),
            width: 200,
            height: 16,
            border_radius: 8
        ) {
            fill (
                bg_color: Color::rgb(88, 166, 255),
                width: 100,
                height: 16,
                border_radius: 8
            ) {}
            thumb (
                bg_color: Color::rgb(255, 255, 255),
                position: Position::Absolute,
                left: 92,
                top: 0,
                width: 16,
                height: 16,
                border_radius: 8
            ) {}
        }
    };

    app.world
        .insert(slider_track, Slider::new(Fixed::ZERO, Fixed::from_int(100)));
    app.world
        .insert(slider_track, SliderTrackWidth(Fixed::from_int(200)));
    app.world.insert(
        slider_track,
        GestureHandler {
            on_gesture: slider_handler,
        },
    );

    // --- Switch ---
    let switch_track = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        switch_track (
            bg_color: Color::rgb(80, 80, 100),
            width: 50,
            height: 26,
            border_radius: 13
        ) {
            sw_thumb (
                bg_color: Color::rgb(255, 255, 255),
                position: Position::Absolute,
                left: 3,
                top: 3,
                width: 20,
                height: 20,
                border_radius: 10
            ) {}
        }
    };

    app.world.insert(switch_track, Switch::new());
    app.world.insert(
        switch_track,
        GestureHandler {
            on_gesture: switch_handler,
        },
    );

    app.set_root(root);
    app.add_plugin(StdInstantClockPlugin::default())
        .add_plugin(FpsSummaryPlugin::default());
    app.run();
}
