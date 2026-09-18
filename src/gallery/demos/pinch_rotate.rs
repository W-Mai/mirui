extern crate alloc;

#[cfg(feature = "std")]
use crate::anim::ease;
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
#[cfg(feature = "std")]
use crate::input::event::sim::{SimAction, SimTimeline, sim_timeline_system};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::types::DimPoint;
use crate::types::{Fixed64, Transform};
use crate::ui::icons::ICON_PLUS;
use crate::ui::theme::ThemedColor;
use crate::ui::widgets::icon::Icon;
use crate::ui::widgets::{ParagraphStyle, Text};
use alloc::format;
#[cfg(feature = "std")]
use alloc::vec;

pub const DEFAULT_VIEW: (u16, u16) = (480, 360);

const BASE_W: i32 = 160;
const BASE_H: i32 = 120;

pub struct PinchTarget {
    status: Signal<PinchStatus>,
    pub last_pinch: Fixed64,
    pub last_rotate: Fixed,
    pub visual_scale: Fixed,
    pub visual_scale64: Fixed64,
    pub visual_rotation: Fixed,
    pub pinch_events: u32,
    pub rotate_events: u32,
    pub mode: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PinchStatus {
    mode: &'static str,
    scale_pct: i32,
    rotation_deg: i32,
    pinch_events: u32,
    rotate_events: u32,
}

impl PinchStatus {
    const IDLE: Self = Self {
        mode: "IDLE",
        scale_pct: 100,
        rotation_deg: 0,
        pinch_events: 0,
        rotate_events: 0,
    };

    fn label(self) -> alloc::string::String {
        format!(
            "{} · {}% · {} DEG · P{} R{}",
            self.mode, self.scale_pct, self.rotation_deg, self.pinch_events, self.rotate_events
        )
    }
}

fn refresh(world: &mut World, entity: Entity) {
    let snapshot = world.get::<PinchTarget>(entity).map(|t| {
        (
            t.status.clone(),
            t.mode,
            t.visual_scale,
            t.visual_rotation,
            t.pinch_events,
            t.rotate_events,
        )
    });
    let Some((status, mode, visual_scale, visual_rotation, pinch_events, rotate_events)) = snapshot
    else {
        return;
    };

    let visual_rot_deg = visual_rotation * Fixed::from_int(180) / Fixed::PI;
    let xform = Transform::scale(visual_scale, visual_scale)
        .compose(&Transform::rotate_deg(visual_rot_deg));
    crate::ui::widgets::set_transform(world, entity, xform);

    let visual_scale_pct = (visual_scale * Fixed::from_int(100)).to_int();
    let visual_rot_int = visual_rot_deg.to_int();
    status.set(PinchStatus {
        mode,
        scale_pct: visual_scale_pct,
        rotation_deg: visual_rot_int,
        pinch_events,
        rotate_events,
    });
}

#[compose]
pub fn build_widgets() {
    let status = Signal::new(PinchStatus::IDLE);
    let status_text = status.clone();
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(16),
            row_gap: 12,
            bg_color: ColorToken::Surface
        ) {
            View (
                width: Dimension::percent(100),
                height: 32,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 16
            ) {
                Text (
                    text: ${ status_text.get().label() },
                    grow: 1.0,
                    width: Dimension::percent(100),
                    height: 32,
                    font_size: 10,
                    text_color: ColorToken::Secondary,
                    paragraph: ParagraphStyle::label(),
                    id: "pinch_status"
                )
            }
            View (
                grow: 1.0,
                width: Dimension::percent(100),
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {
                pinch_target (status)
            }
            Text (
                "TWO-POINTER GESTURE · LIVE TRANSFORM",
                width: Dimension::percent(100),
                height: 20,
                font_size: 9,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label()
            )
        }
    };
}

#[compose]
fn pinch_target(status: Signal<PinchStatus>) -> Entity {
    //~focus-start
    ui! {
        View (
            id: "pinch_target",
            width: BASE_W,
            height: BASE_H,
            bg_color: ColorToken::Primary,
            border_color: ColorToken::OnPrimary,
            border_width: 2,
            border_radius: 28
        ) [
            PinchTarget {
                status,
                last_pinch: Fixed64::ONE,
                last_rotate: Fixed::ZERO,
                visual_scale: Fixed::ONE,
                visual_scale64: Fixed64::ONE,
                visual_rotation: Fixed::ZERO,
                pinch_events: 0,
                rotate_events: 0,
                mode: "IDLE",
            },
        ] on Pinch {
            if let Some(t) = ctx.world.get_mut::<PinchTarget>(ctx.entity) {
                t.last_pinch = *scale_delta;
                let lo = Fixed64::from_ratio(65, 100);
                let hi = Fixed64::from_ratio(8, 5);
                t.visual_scale64 = (t.visual_scale64 * *scale_delta).clamp(lo, hi);
                t.visual_scale = t.visual_scale64.to_fixed();
                t.pinch_events += 1;
                t.mode = if *scale_delta > Fixed64::ONE {
                    "EXPAND"
                } else if *scale_delta < Fixed64::ONE {
                    "SHRINK"
                } else {
                    "PINCH"
                };
            }
            refresh(ctx.world, ctx.entity);
        } on Rotate {
            if let Some(t) = ctx.world.get_mut::<PinchTarget>(ctx.entity) {
                t.last_rotate = *angle;
                t.visual_rotation += *angle;
                t.rotate_events += 1;
                t.mode = "ROTATE";
            }
            refresh(ctx.world, ctx.entity);
        }
        {
            Icon (
                path: ICON_PLUS,
                color: ThemedColor::Token(ColorToken::OnPrimary),
                size: Dimension::Px(Fixed::from_int(46)),
                grow: 1.0,
                width: Dimension::percent(100)
            )
        }
    }
    //~focus-end
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.compose(parent, build_widgets);

    let target = app
        .world
        .find_by_id("pinch_target")
        .expect("pinch target must be composed before its timeline");
    let small = Fixed::from_int(40);
    let large = Fixed::from_int(80);
    let radius = Fixed::from_int(50);
    let timeline = SimTimeline::new(vec![
        SimAction::pinch(
            DimPoint::CENTER,
            small,
            large,
            1500,
            ease::ease_in_out_cubic,
        )
        .on(target),
        SimAction::wait(800),
        SimAction::pinch(
            DimPoint::CENTER,
            large,
            small,
            1500,
            ease::ease_in_out_cubic,
        )
        .on(target),
        SimAction::wait(800),
        SimAction::pinch(
            DimPoint::CENTER,
            small,
            large,
            1500,
            ease::ease_in_out_cubic,
        )
        .on(target),
        SimAction::wait(800),
        SimAction::rotate_gesture(
            DimPoint::CENTER,
            radius,
            Fixed::ZERO,
            Fixed::PI / Fixed::from_int(2),
            1500,
            ease::ease_in_out_cubic,
        )
        .on(target),
        SimAction::wait(800),
        SimAction::rotate_gesture(
            DimPoint::CENTER,
            radius,
            Fixed::PI / Fixed::from_int(2),
            Fixed::ZERO,
            1500,
            ease::ease_in_out_cubic,
        )
        .on(target),
        SimAction::wait(800),
        SimAction::pinch(
            DimPoint::CENTER,
            large,
            small,
            1500,
            ease::ease_in_out_cubic,
        )
        .on(target),
        SimAction::wait(800),
    ])
    .looping(true);
    app.world.insert_resource(timeline);
    app.add_system(sim_timeline_system::system());
    app.add_plugin(StdInstantClockPlugin);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::flush_signal_dirty;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::UiScope;
    use crate::ui::widgets::WidgetTransform;

    use crate::input::event::GestureHandler;
    use crate::input::event::gesture::GestureEvent;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }

    #[test]
    fn pinch_updates_target_and_status() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);
        let target = world.find_by_id("pinch_target").expect("target id");
        let status = world.find_by_id("pinch_status").expect("status id");

        assert_eq!(
            world.get::<PinchTarget>(target).map(|t| t.pinch_events),
            Some(0)
        );
        GestureHandler::trigger(
            &mut world,
            target,
            &GestureEvent::Pinch {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                scale_delta: Fixed64::from_int(2),
                target,
            },
        );
        assert_eq!(
            world.get::<PinchTarget>(target).map(|t| t.pinch_events),
            Some(1)
        );
        assert_eq!(
            world.get::<PinchTarget>(target).map(|t| t.mode),
            Some("EXPAND")
        );
        flush_signal_dirty(&mut world);
        assert!(
            world
                .get::<Text>(status)
                .expect("status text")
                .resolve(&world)
                .contains("EXPAND")
        );
        assert!(world.has::<WidgetTransform>(target));
        assert_eq!(
            world.get::<Text>(status).map(Text::paragraph),
            Some(&ParagraphStyle::label())
        );
    }

    #[test]
    fn portrait_layout_reflows_the_target_between_status_and_footer() {
        use crate::types::Viewport;
        use crate::ui::ComputedRect;
        use crate::ui::render_system::update_layout;

        let mut app = App::headless(360, 480);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        app.compose(root, build_widgets);
        app.set_root(root);
        update_layout(&mut app.world, root, &Viewport::new(360, 480, Fixed::ONE));

        let target = app.world.find_by_id("pinch_target").expect("target id");
        let status = app.world.find_by_id("pinch_status").expect("status id");
        let target_rect = app
            .world
            .get::<ComputedRect>(target)
            .expect("target rect")
            .0;
        let status_rect = app
            .world
            .get::<ComputedRect>(status)
            .expect("status rect")
            .0;

        assert_eq!(status_rect.x.to_int(), 16);
        assert_eq!(status_rect.w.to_int(), 328);
        assert_eq!(target_rect.x.to_int(), 100);
        assert!(target_rect.y > status_rect.y + status_rect.h);
        assert!(target_rect.y + target_rect.h < Fixed::from_int(444));
    }

    #[test]
    fn portrait_status_uses_the_compact_vocabulary() {
        let label = PinchStatus {
            mode: "ROTATE",
            scale_pct: 158,
            rotation_deg: 25,
            pinch_events: 791,
            rotate_events: 3,
        }
        .label();

        assert_eq!(label, "ROTATE · 158% · 25 DEG · P791 R3");
    }
}
