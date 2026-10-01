use crate::prelude::*;
use crate::ui::ComputedRect;
use crate::ui::widgets::{DropGlow, DropShadow, WidgetTransform};

pub(super) const BLUR_OVERLAY_WIDTH_PERCENT: i32 = 58;

#[crate::component]
pub struct ColorFlash {
    pub frame: u32,
}

#[system(order = Animation)]
pub fn animate_color_flash(flashes: query!(mut ColorFlash, mut Style)) {
    for (flash, style) in flashes {
        flash.frame = flash.frame.wrapping_add(1);
        let frame = flash.frame;
        let color = match (frame / 60) % 3 {
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
