use super::state::{ConsoleModel, ConsoleMotion};
use super::style::{BORDER, MUTED, PANEL, TEXT};
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::ui::Theme;
use crate::ui::view::PaintCtx;

#[crate::component]
pub(super) struct KineticOrbit {
    pub(super) model: model!(ConsoleModel),
    pub(super) motion: model!(ConsoleMotion),
}

#[crate::component]
pub(super) struct KineticWave {
    pub(super) model: model!(ConsoleModel),
    pub(super) motion: model!(ConsoleMotion),
}

struct InstrumentPainter<'paint, 'frame> {
    paint: &'paint mut PaintCtx<'frame>,
}

impl InstrumentPainter<'_, '_> {
    fn fill(&mut self, area: Rect, color: Color, radius: Fixed, opacity: u8) {
        self.paint.draw(&DrawCommand::Fill {
            area,
            transform: self.paint.transform(),
            quad: None,
            color,
            radius,
            opa: opacity,
        });
    }
}

#[crate::view(name = "KineticOrbit", priority = 60)]
pub(super) fn orbit_render(
    orbit: component!(KineticOrbit),
    model: model!(orbit.model, watch(visual_revision)),
    motion: model!(orbit.motion, watch(orbit_render_key)),
    paint: paint!(),
    theme: res!(Theme),
) {
    let phase = motion.phase.phase();
    let panel = theme.resolve(PANEL);
    let border = theme.resolve(BORDER);
    let text = theme.resolve(TEXT);
    let accent = theme.resolve(model.mode.accent());
    let secondary = theme.resolve(model.mode.secondary());

    paint.set_background_handled(true);
    let rect = paint.rect();
    let mut painter = InstrumentPainter { paint };
    painter.fill(rect, panel, Fixed::ZERO, 255);

    let grid = panel.blend_with(border, Fixed::from_ratio(1, 2));
    for step in 1..4 {
        let x = rect.x + rect.w * Fixed::from_ratio(step, 4);
        painter.fill(
            Rect::new(
                x,
                rect.y + Fixed::ONE,
                Fixed::ONE,
                rect.h - Fixed::from_int(2),
            ),
            grid,
            Fixed::ZERO,
            130,
        );
    }
    for step in 1..3 {
        let y = rect.y + rect.h * Fixed::from_ratio(step, 3);
        painter.fill(
            Rect::new(
                rect.x + Fixed::ONE,
                y,
                rect.w - Fixed::from_int(2),
                Fixed::ONE,
            ),
            grid,
            Fixed::ZERO,
            130,
        );
    }

    let center = Point {
        x: rect.x + rect.w / Fixed::from_int(2),
        y: rect.y + rect.h * Fixed::from_ratio(9, 20),
    };
    let base_radius = rect.w.min(rect.h) * Fixed::from_ratio(7, 20);
    for ring in 0..3 {
        let radius = base_radius - Fixed::from_int(ring * 5);
        let node_angle = phase * Fixed::from_int(ring + 2) + Fixed::from_int(ring * 73);
        for trail in 1..=4 {
            let angle = node_angle - Fixed::from_int(trail * (8 + ring * 2));
            let point = Point {
                x: center.x + Fixed::cos_deg(angle) * radius,
                y: center.y + Fixed::sin_deg(angle) * radius,
            };
            let size = if ring == model.focused as i32 && trail <= 2 {
                Fixed::from_int(2)
            } else {
                Fixed::ONE
            };
            painter.fill(
                Rect::new(point.x - size / 2, point.y - size / 2, size, size),
                if ring == model.focused as i32 {
                    accent
                } else {
                    secondary
                },
                Fixed::ZERO,
                230u8.saturating_sub((trail * 27) as u8),
            );
        }
        let node = Point {
            x: center.x + Fixed::cos_deg(node_angle) * radius,
            y: center.y + Fixed::sin_deg(node_angle) * radius,
        };
        let size = if ring == model.focused as i32 { 5 } else { 3 };
        painter.fill(
            Rect::new(
                node.x - Fixed::from_ratio(size, 2),
                node.y - Fixed::from_ratio(size, 2),
                Fixed::from_int(size),
                Fixed::from_int(size),
            ),
            if ring == model.focused as i32 {
                text
            } else {
                secondary
            },
            Fixed::from_int(size),
            255,
        );
    }

    for size in [13, 8, 3] {
        let color = if size == 13 {
            Color::rgba(accent.r, accent.g, accent.b, 70)
        } else if size == 8 {
            secondary
        } else {
            text
        };
        painter.fill(
            Rect::new(
                center.x - Fixed::from_ratio(size, 2),
                center.y - Fixed::from_ratio(size, 2),
                Fixed::from_int(size),
                Fixed::from_int(size),
            ),
            color,
            Fixed::from_int(size),
            255,
        );
    }
}

#[crate::view(name = "KineticWave", priority = 61)]
pub(super) fn wave_render(
    wave: component!(KineticWave),
    model: model!(wave.model, watch(visual_revision)),
    motion: model!(wave.motion, watch(wave_render_key)),
    paint: paint!(),
    theme: res!(Theme),
) {
    let phase = motion.phase.phase();
    let panel = theme.resolve(PANEL);
    let muted = theme.resolve(MUTED);
    let accent = theme.resolve(model.mode.accent());
    paint.set_background_handled(true);
    let rect = paint.rect();
    let mut painter = InstrumentPainter { paint };
    painter.fill(rect, panel, Fixed::ZERO, 255);

    let plot_inset = Fixed::from_int(5);
    let bar_width = Fixed::from_int(2);
    let plot_y = rect.y + rect.h - plot_inset;
    let plot_span = rect.w - plot_inset * Fixed::from_int(2) - bar_width;
    for bar in 0..13 {
        let wave = Fixed::sin_deg(phase * Fixed::from_int(2) + Fixed::from_int(bar * 37));
        let height = Fixed::from_int(2)
            + wave.abs() * Fixed::from_int(7) * model.intensity / Fixed::from_int(100);
        let x = rect.x + plot_inset + plot_span * Fixed::from_ratio(bar, 12);
        painter.fill(
            Rect::new(x, plot_y - height, bar_width, height),
            if bar % 3 == model.focused as i32 {
                accent
            } else {
                muted
            },
            Fixed::ONE,
            220,
        );
    }
}
