use super::state::IrisState;
use super::style::{BORDER, CLIP, MUTED, SEE, TEXT, is_dark};
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::ui::Theme;
use crate::ui::view::PaintCtx;

const HISTOGRAM_COLUMNS: i32 = 48;
/// Focus distances marked on the scale, in centimetres; 0 is infinity.
pub(super) const FOCUS_MARKS: [i32; 5] = [50, 100, 200, 500, 0];
const NEAREST_FOCUS: i32 = 45;

fn fill(paint: &mut PaintCtx<'_>, area: Rect, color: Color, radius: Fixed, opa: u8) {
    let transform = paint.transform();
    paint.draw(&DrawCommand::Fill {
        area,
        transform,
        quad: None,
        color,
        radius,
        opa,
    });
}

/// Position of a focus distance on the scale: linear in dioptres, so the
/// near end spreads out and infinity sits at the far edge.
pub(super) fn focus_position(centimetres: Fixed) -> Fixed {
    if centimetres <= Fixed::ZERO {
        return Fixed::ONE;
    }
    let near = Fixed::from_int(NEAREST_FOCUS);
    (Fixed::ONE - near / centimetres.max(near))
        .max(Fixed::ZERO)
        .min(Fixed::ONE)
}

/// Where a distance sits on the drawn track: finite distances use the first
/// 86% so the infinity mark keeps its own place at the far end.
pub(super) fn focus_track(centimetres: Fixed) -> Fixed {
    if centimetres <= Fixed::ZERO {
        Fixed::from_ratio(97, 100)
    } else {
        focus_position(centimetres) * Fixed::from_ratio(86, 100)
    }
}

fn bell(x: Fixed, center: Fixed, width: Fixed) -> Fixed {
    let d = (x - center) / width;
    let q = Fixed::ONE + d * d;
    Fixed::ONE / (q * q)
}

/// Share of pixels at luminance `x` for the simulated scene: deep shadows,
/// a broad midtone body and a bright sky, all shifted by the exposure.
pub(super) fn luminance(x: Fixed, exposure: Fixed) -> Fixed {
    let shift = exposure * Fixed::from_ratio(9, 100);
    (bell(
        x,
        Fixed::from_ratio(44, 100) + shift,
        Fixed::from_ratio(16, 100),
    ) + bell(
        x,
        Fixed::from_ratio(76, 100) + shift,
        Fixed::from_ratio(7, 100),
    ) * Fixed::from_ratio(42, 100)
        + bell(
            x,
            Fixed::from_ratio(10, 100) + shift / 2,
            Fixed::from_ratio(6, 100),
        ) * Fixed::from_ratio(24, 100))
    .min(Fixed::ONE)
}

/// Luminance histogram: one neutral channel, quarter-tone grid lines and a
/// clipping marker at each end.
#[crate::component]
pub(super) struct Histogram {
    pub(super) model: model!(IrisState),
}

impl Histogram {
    fn render(&self, paint: &mut PaintCtx<'_>, state: &IrisState, theme: &Theme) {
        let rect = paint.rect();
        let left = rect.x;
        let width = rect.w;
        let top = rect.y + Fixed::from_int(4);
        let height = rect.h - Fixed::from_int(4);
        if width <= Fixed::ZERO || height <= Fixed::ZERO {
            return;
        }
        let grid = theme.resolve(BORDER);
        let ink = theme.resolve(TEXT);
        for quarter in 1..4 {
            fill(
                paint,
                Rect::new(
                    left + width * Fixed::from_ratio(quarter, 4),
                    top,
                    Fixed::ONE,
                    height,
                ),
                grid,
                Fixed::ZERO,
                255,
            );
        }
        fill(
            paint,
            Rect::new(left, top + height - Fixed::ONE, width, Fixed::ONE),
            grid,
            Fixed::ZERO,
            255,
        );
        let exposure = state.exposure();
        let pitch = width / HISTOGRAM_COLUMNS;
        let opacity = if is_dark(theme) { 92 } else { 74 };
        for index in 0..HISTOGRAM_COLUMNS {
            let x = Fixed::from_ratio(index * 2 + 1, HISTOGRAM_COLUMNS * 2);
            let h = (height * luminance(x, exposure) * Fixed::from_ratio(92, 100)).max(Fixed::ONE);
            // Columns overlap by half a pixel so the area reads as one shape.
            fill(
                paint,
                Rect::new(
                    left + pitch * index,
                    top + height - h,
                    pitch + Fixed::HALF,
                    h,
                ),
                ink,
                Fixed::ZERO,
                opacity,
            );
        }
        let marker = Fixed::from_int(6);
        let shadows = exposure < Fixed::from_int(-2);
        let highlights = exposure > Fixed::from_ratio(3, 2);
        for (x, clipped) in [(left, shadows), (left + width - marker, highlights)] {
            fill(
                paint,
                Rect::new(x, rect.y, marker, marker),
                if clipped {
                    theme.resolve(CLIP)
                } else {
                    theme.resolve(MUTED)
                },
                Fixed::ONE,
                if clipped { 255 } else { 90 },
            );
        }
    }
}

#[crate::view(component = Histogram, name = "IrisHistogram", priority = 60)]
pub(super) fn histogram_render(
    component: component!(),
    model: model!(component.model, watch(exposure)),
    paint: paint!(),
    theme: res!(Theme),
) {
    component.render(paint, model, &theme);
}

/// Exposure level indicator from -3 to +3 EV in third stops.
#[crate::component]
pub(super) struct ExposureMeter {
    pub(super) model: model!(IrisState),
}

impl ExposureMeter {
    fn render(&self, paint: &mut PaintCtx<'_>, state: &IrisState, theme: &Theme) {
        let rect = paint.rect();
        let inset = Fixed::from_int(6);
        let left = rect.x + inset;
        let width = rect.w - inset * 2;
        if width <= Fixed::from_int(20) {
            return;
        }
        let mid = rect.y + rect.h / 2;
        let muted = theme.resolve(MUTED);
        let text = theme.resolve(TEXT);
        for tick in 0..19 {
            let x = left + width * Fixed::from_ratio(tick, 18);
            let major = tick % 3 == 0;
            let h = if tick == 9 {
                Fixed::from_int(12)
            } else if major {
                Fixed::from_int(8)
            } else {
                Fixed::from_int(4)
            };
            fill(
                paint,
                Rect::new(x - Fixed::HALF, mid - h / 2, Fixed::ONE, h),
                if tick == 9 { text } else { muted },
                Fixed::ZERO,
                if major { 220 } else { 150 },
            );
        }
        // Camera level indicators draw the offset in the text colour; only the
        // tick under the reading is emphasised.
        let exposure = state.exposure();
        let zero = left + width / 2;
        let at = zero + width / 2 * exposure / 3;
        let (from, to) = if at < zero { (at, zero) } else { (zero, at) };
        fill(
            paint,
            Rect::new(
                from,
                mid + Fixed::from_int(8),
                (to - from).max(Fixed::ONE),
                Fixed::from_int(3),
            ),
            text,
            Fixed::from_ratio(3, 2),
            200,
        );
        fill(
            paint,
            Rect::new(
                at - Fixed::from_int(4),
                mid - Fixed::from_int(15),
                Fixed::from_int(8),
                Fixed::from_int(4),
            ),
            text,
            Fixed::from_int(2),
            255,
        );
    }
}

#[crate::view(component = ExposureMeter, name = "IrisExposureMeter", priority = 60)]
pub(super) fn exposure_render(
    component: component!(),
    model: model!(component.model, watch(exposure)),
    paint: paint!(),
    theme: res!(Theme),
) {
    component.render(paint, model, &theme);
}

/// Focus distance ruler with a needle on the current distance.
#[crate::component]
pub(super) struct FocusScale {
    pub(super) model: model!(IrisState),
}

impl FocusScale {
    fn render(&self, paint: &mut PaintCtx<'_>, state: &IrisState, theme: &Theme) {
        let rect = paint.rect();
        let left = rect.x;
        let width = rect.w;
        let base = rect.y + Fixed::from_int(4);
        let muted = theme.resolve(MUTED);
        fill(
            paint,
            Rect::new(left, base, width, Fixed::ONE),
            muted,
            Fixed::ZERO,
            140,
        );
        // Minor ticks are even in dioptres; the marked distances get long ticks.
        for step in 0..=24 {
            let x = left + width * Fixed::from_ratio(step * 86, 2400);
            fill(
                paint,
                Rect::new(x - Fixed::HALF, base, Fixed::ONE, Fixed::from_int(4)),
                muted,
                Fixed::ZERO,
                110,
            );
        }
        for mark in FOCUS_MARKS {
            let x = left + width * focus_track(Fixed::from_int(mark));
            fill(
                paint,
                Rect::new(x - Fixed::HALF, base, Fixed::ONE, Fixed::from_int(9)),
                muted,
                Fixed::ZERO,
                230,
            );
        }
        let x = left + width * focus_track(state.focus * 100);
        fill(
            paint,
            Rect::new(
                x - Fixed::ONE,
                base - Fixed::from_int(4),
                Fixed::from_int(2),
                Fixed::from_int(16),
            ),
            theme.resolve(SEE),
            Fixed::ONE,
            255,
        );
    }
}

#[crate::view(component = FocusScale, name = "IrisFocusScale", priority = 60)]
pub(super) fn focus_render(
    component: component!(),
    model: model!(component.model, watch(focus)),
    paint: paint!(),
    theme: res!(Theme),
) {
    component.render(paint, model, &theme);
}

/// Shutter button: a ring around a red disc that dips while the blades close.
#[crate::component]
pub(super) struct ShutterRing {
    pub(super) model: model!(IrisState),
}

impl ShutterRing {
    fn render(&self, paint: &mut PaintCtx<'_>, state: &IrisState, theme: &Theme) {
        let rect = paint.rect();
        let size = rect.w.min(rect.h);
        let center = Point::new(rect.x + rect.w / 2, rect.y + rect.h / 2);
        let radius = size / 2 - Fixed::from_int(2);
        let ring = theme.resolve(TEXT);
        let transform = paint.transform();
        paint.draw(&DrawCommand::Arc {
            center,
            transform,
            radius,
            start_angle: Fixed::ZERO,
            end_angle: Fixed::from_int(360),
            color: ring,
            width: Fixed::from_int(3).max(size / 24),
            opa: 235,
        });
        let press = state.closure;
        let disc = radius * (Fixed::from_ratio(80, 100) - press * Fixed::from_ratio(12, 100));
        fill(
            paint,
            Rect::new(center.x - disc, center.y - disc, disc * 2, disc * 2),
            theme.resolve(SEE),
            disc,
            255,
        );
    }
}

#[crate::view(component = ShutterRing, name = "IrisShutterRing", priority = 60)]
pub(super) fn shutter_render(
    component: component!(),
    model: model!(component.model, watch(closure)),
    paint: paint!(),
    theme: res!(Theme),
) {
    component.render(paint, model, &theme);
}

/// Exposure arc around a round face: the bezel version of the meter.
#[crate::component]
pub(super) struct DialMeter {
    pub(super) model: model!(IrisState),
}

impl DialMeter {
    fn render(&self, paint: &mut PaintCtx<'_>, state: &IrisState, theme: &Theme) {
        let rect = paint.rect();
        let center = Point::new(rect.x + rect.w / 2, rect.y + rect.h / 2);
        let radius = rect.w.min(rect.h) / 2 - Fixed::from_int(12);
        let transform = paint.transform();
        let arc = |paint: &mut PaintCtx<'_>,
                   from: Fixed,
                   to: Fixed,
                   color: Color,
                   width: Fixed,
                   opa: u8| {
            paint.draw(&DrawCommand::Arc {
                center,
                transform,
                radius,
                start_angle: from,
                end_angle: to,
                color,
                width,
                opa,
            });
        };
        // The scale spans 60° on either side of the bottom of the face.
        let zero = Fixed::from_int(90);
        arc(
            paint,
            zero - Fixed::from_int(60),
            zero + Fixed::from_int(60),
            theme.resolve(BORDER),
            Fixed::from_int(3),
            255,
        );
        let exposure = state.exposure();
        let at = zero - exposure * 20;
        let (from, to) = if at < zero { (at, zero) } else { (zero, at) };
        arc(
            paint,
            from,
            to.max(from + Fixed::ONE),
            theme.resolve(TEXT),
            Fixed::from_int(3),
            200,
        );
        let dot = Point::new(
            center.x + Fixed::cos_deg(at) * radius,
            center.y + Fixed::sin_deg(at) * radius,
        );
        fill(
            paint,
            Rect::new(
                dot.x - Fixed::from_int(4),
                dot.y - Fixed::from_int(4),
                Fixed::from_int(8),
                Fixed::from_int(8),
            ),
            theme.resolve(TEXT),
            Fixed::from_int(4),
            255,
        );
        for tick in [-3, 0, 3] {
            let angle = zero - Fixed::from_int(tick * 20);
            let inner = radius - Fixed::from_int(7);
            let a = Point::new(
                center.x + Fixed::cos_deg(angle) * inner,
                center.y + Fixed::sin_deg(angle) * inner,
            );
            fill(
                paint,
                Rect::new(
                    a.x - Fixed::ONE,
                    a.y - Fixed::ONE,
                    Fixed::from_int(2),
                    Fixed::from_int(2),
                ),
                theme.resolve(MUTED),
                Fixed::ONE,
                255,
            );
        }
    }
}

#[crate::view(component = DialMeter, name = "IrisDialMeter", priority = 55)]
pub(super) fn dial_render(
    component: component!(),
    model: model!(component.model, watch(exposure)),
    paint: paint!(),
    theme: res!(Theme),
) {
    component.render(paint, model, &theme);
}
