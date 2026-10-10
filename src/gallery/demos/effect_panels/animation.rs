use crate::prelude::*;
use crate::ui::ComputedRect;
use crate::ui::widgets::{DropGlow, DropShadow, WidgetTransform};

pub(super) const BLUR_OVERLAY_WIDTH_PERCENT: i32 = 58;

#[crate::component]
pub struct ColorFlash {
    pub frame: u32,
}

#[system(order = Animation)]
pub fn animate_color_flash(
    flashes: query!(Entity, mut ColorFlash; with Style;),
    styles: query!(mut Style),
) {
    for (entity, flash) in flashes {
        let previous_phase = (flash.frame / 60) % 3;
        flash.frame = flash.frame.wrapping_add(1);
        let frame = flash.frame;
        let phase = (frame / 60) % 3;
        if frame != 1 && phase == previous_phase {
            continue;
        }
        let Some(style) = styles.get(entity) else {
            continue;
        };
        let color = match phase {
            0 => Color::rgb(220, 60, 60),
            1 => Color::rgb(60, 200, 80),
            _ => Color::rgb(40, 140, 220),
        };
        style.bg_color = Some(color.into());
    }
}

#[animate(component = BlurPan)]
pub fn apply_blur_pan(
    value: motion!(),
    overlay: component!(ComputedRect),
    transform: component!(mut WidgetTransform),
) {
    let travel = overlay.0.w
        * Fixed::from_ratio(100 - BLUR_OVERLAY_WIDTH_PERCENT, BLUR_OVERLAY_WIDTH_PERCENT);
    transform.0.tx = travel * value;
}

#[animate(component = ShadowOffset)]
pub fn apply_shadow_offset(value: motion!(), shadow: component!(mut DropShadow)) {
    shadow.offset.0 = value;
    shadow.offset.1 = value;
}

#[animate(component = GlowPulse)]
pub fn apply_glow_pulse(value: motion!(), glow: component!(mut DropGlow)) {
    glow.blur_radius = value;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;
    use core::any::TypeId;

    #[test]
    fn color_flash_only_invalidates_at_counter_start_or_phase_change() {
        let mut world = World::new();
        let source = world.spawn_empty();
        world.insert(source, ColorFlash { frame: 0 });
        world.insert(source, Style::default());
        let style_type = TypeId::of::<Style>();
        world.watch_query_dependency_type(style_type);
        let system = animate_color_flash::system();
        for frame in 1..=180 {
            system.run_once(&mut world).unwrap();
            assert_eq!(world.get::<ColorFlash>(source).unwrap().frame, frame);
            let expected = match (frame / 60) % 3 {
                0 => Color::rgb(220, 60, 60),
                1 => Color::rgb(60, 200, 80),
                _ => Color::rgb(40, 140, 220),
            };
            assert_eq!(
                world.get::<Style>(source).unwrap().bg_color,
                Some(expected.into())
            );
            let mut changes = 0;
            world.drain_query_dependency_changes(|_, kind, _| {
                assert_eq!(kind, style_type);
                changes += 1;
            });
            assert_eq!(changes, usize::from(frame == 1 || frame % 60 == 0));
        }
        world.get_mut::<ColorFlash>(source).unwrap().frame = u32::MAX;
        for frame in 0..=2 {
            system.run_once(&mut world).unwrap();
            assert_eq!(world.get::<ColorFlash>(source).unwrap().frame, frame);
            assert_eq!(
                world.get::<Style>(source).unwrap().bg_color,
                Some(Color::rgb(220, 60, 60).into())
            );
            let mut changes = 0;
            world.drain_query_dependency_changes(|_, kind, _| {
                assert_eq!(kind, style_type);
                changes += 1;
            });
            assert_eq!(changes, usize::from(frame <= 1));
        }
    }
}
