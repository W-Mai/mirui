use super::style::*;
use crate::prelude::*;
use crate::render::{command::DrawCommand, raster::FillRule, scene::Paint};
use crate::types::Transform;
use crate::ui::theme::{Theme, ThemedColor};
use crate::ui::view::PaintCtx;

static TREE_CANOPY: Path = path!(
    M 4 0 H 6 V 2 H 7 V 4 H 8 V 6 H 9 V 8 H 10 V 10
    H 0 V 8 H 1 V 6 H 2 V 4 H 3 V 2 H 4 Z
);
static TENT_SHELL: Path = path!(
    M 7 0 H 9 V 1 H 10 V 2 H 11 V 3 H 12 V 4
    H 13 V 5 H 14 V 6 H 15 V 7 H 16 V 8 H 0
    V 7 H 1 V 6 H 2 V 5 H 3 V 4 H 4
    V 3 H 5 V 2 H 6 V 1 H 7 Z
);
static TENT_LIGHT: Path = path!(
    M 5 0 H 6 V 6 H 0 V 5 H 1 V 4 H 2 V 3 H 3 V 2 H 4 V 1 H 5 Z
);

#[derive(Clone, Copy, Debug)]
#[component]
pub(super) enum Scenery {
    Logo,
    Header,
    Map,
    Margins,
    Landscape,
    Ridge,
}

struct Painter<'a, F> {
    theme: &'a Theme,
    emit: F,
    width: i32,
    height: i32,
    unit: i32,
}

impl<F: FnMut(DrawCommand<'_>)> Painter<'_, F> {
    fn contour(
        &mut self,
        path: &Path,
        x: i32,
        y: i32,
        scale: i32,
        color: impl Into<ThemedColor>,
    ) -> bool {
        let color = color.into().resolve(self.theme);
        // Preserve Fill's whole-pixel coverage and translucent blending on reduced grids.
        if scale % self.unit != 0 || color.a != 255 {
            return false;
        }
        let scale = Fixed::from_int(scale / self.unit);
        (self.emit)(DrawCommand::FillPath {
            path,
            transform: Transform::translate(
                Fixed::from_int(x / self.unit),
                Fixed::from_int(y / self.unit),
            )
            .compose(&Transform::scale(scale, scale)),
            paint: &Paint::Color(color.into()),
            opa: 255,
            fill_rule: FillRule::NonZero,
        });
        true
    }

    fn tree(&mut self, x: i32, y: i32, scale: i32, color: impl Into<ThemedColor> + Copy) {
        self.fill(x - scale, y - 3 * scale, 2 * scale, 3 * scale, DEEP);
        if !self.contour(&TREE_CANOPY, x - 5 * scale, y - 12 * scale, scale, color) {
            for row in 0..5 {
                self.fill(
                    x - (row + 1) * scale,
                    y - (12 - row * 2) * scale,
                    (row * 2 + 2) * scale,
                    2 * scale,
                    color,
                );
            }
        }
    }
    fn tent(&mut self, x: i32, y: i32, scale: i32) {
        if !self.contour(&TENT_SHELL, x - 8 * scale, y - 8 * scale, scale, CLAY) {
            for row in 0..8 {
                self.fill(
                    x - (row + 1) * scale,
                    y - (8 - row) * scale,
                    (row * 2 + 2) * scale,
                    scale,
                    CLAY,
                );
            }
        }
        if !self.contour(&TENT_LIGHT, x - 6 * scale, y - 6 * scale, scale, WARNING) {
            for row in 0..6 {
                self.fill(
                    x - (row + 1) * scale,
                    y - (6 - row) * scale,
                    (row + 1) * scale,
                    scale,
                    WARNING,
                );
            }
        }
        self.fill(x, y - 4 * scale, 3 * scale, 4 * scale, DEEP);
        self.fill(x - 9 * scale, y, 18 * scale, scale, DEEP);
    }
    fn mountain(
        &mut self,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        color: impl Into<ThemedColor> + Copy,
    ) {
        for row in (0..height).step_by(4) {
            let span = (width * (row + 4).min(height) / height).min(self.width);
            self.fill(
                (x - span / 2).clamp(0, self.width - span),
                y - height + row,
                span,
                4.min(height - row),
                color,
            );
        }
    }
    fn scenery(&mut self, kind: Scenery) {
        let (w, h) = (self.width, self.height);
        match kind {
            Scenery::Logo => {
                let scale = (w.min(h) / 24).max(1);
                self.fill(0, 0, w, h, BACKGROUND);
                self.fill(4, 4, w - 8, h - 8, SKY);
                self.tree(w - 5 * scale, h - 4 * scale, scale, ACCENT);
                self.tent(w / 2 - 2 * scale, h - 5 * scale, scale);
                self.fill(4, h - 5, w - 8, 2, ACCENT);
            }
            Scenery::Header => {
                self.mountain(w - w / 4, h, w / 3, (h - 4).min(48), self.alpha(HILL, 45));
                self.mountain(w - w / 8, h, w / 4, (h - 4).min(64), self.alpha(SKY, 30));
                let scale = (h / 24).clamp(1, 3).min((w / 24).max(1));
                self.tree(w - 8 * scale, h, scale, self.alpha(HILL, 55));
                self.tree(w * 2 / 3, h, 1, self.alpha(HILL, 55));
            }
            Scenery::Map | Scenery::Margins => {
                if matches!(kind, Scenery::Margins) {
                    let (edge, length, stride) = if w < 500 { (4, 4, 16) } else { (10, 8, 22) };
                    for y in (h / 4..h - 36).step_by(stride) {
                        self.fill(edge, y, length, 2, self.alpha(DIVIDER, 125));
                        self.fill(
                            w - edge - length,
                            y + 8,
                            length,
                            2,
                            self.alpha(DIVIDER, 125),
                        );
                    }
                }
                for y in (12..h - 4).step_by(38) {
                    for x in (8..w - 8).step_by(94) {
                        self.fill(x, y, 2, 2, self.alpha(DIVIDER, 70));
                        self.fill(x + 4, y, 2, 2, self.alpha(DIVIDER, 70));
                    }
                }
            }
            Scenery::Ridge => {
                let floor = h - 6;
                self.mountain(w * 3 / 5, floor, w / 3, (floor - 4).min(42), PANEL);
                self.mountain(w * 4 / 5, floor, w / 3, (floor - 4).min(30), HILL);
                self.tent(w * 2 / 3, floor, 1);
                self.tree(w - 16, floor, 1, ACCENT);
                self.tree(w - 32, floor, 1, HILL);
                self.fill(w / 2, floor, w - w / 2, 3, DIVIDER);
            }
            Scenery::Landscape => {
                let floor = h - 10;
                self.fill(0, 0, w, floor, SKY);
                self.mountain(w / 3, floor, w * 2 / 3, (floor - 4).min(60), PANEL);
                self.mountain(w * 3 / 4, floor, w / 2, (floor - 4).min(44), HILL);
                self.tree(20, floor, 2, ACCENT);
                self.tree(w - 20, floor, 2, ACCENT);
                self.tree(w - 38, floor, 1, HILL);
                self.fill(0, floor, w, 4, HILL);
                for x in (10..w - 6).step_by(32) {
                    self.fill(x, floor + 6, 6, 2, DIVIDER);
                }
                self.fill(20, 14, 32, 3, BACKGROUND);
                self.fill(28, 11, 16, 3, BACKGROUND);
            }
        }
    }
    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, color: impl Into<ThemedColor>) {
        if w <= 0 || h <= 0 {
            return;
        }
        #[cfg(test)]
        assert!(
            x >= 0 && y >= 0 && x + w <= self.width && y + h <= self.height,
            "Sprite primitive ({x}, {y}, {w}, {h}) exceeds {}×{}",
            self.width,
            self.height
        );
        (self.emit)(DrawCommand::Fill {
            area: Rect::new(
                x / self.unit,
                y / self.unit,
                (w / self.unit).max(1),
                (h / self.unit).max(1),
            ),
            color: color.into().resolve(self.theme),
            transform: Transform::IDENTITY,
            quad: None,
            radius: Fixed::ZERO,
            opa: 255,
        });
    }
    fn alpha(&self, token: ColorToken, alpha: u8) -> Color {
        let color = self.theme.resolve(token);
        Color::rgba(color.r, color.g, color.b, alpha)
    }
}
#[view(component = Scenery, priority = 50)]
pub(super) fn scenery(component: component!(), paint: paint!(), theme: res!(Theme)) {
    let rect = paint.rect();
    draw_scenery(
        *component,
        rect.w.to_int(),
        rect.h.to_int(),
        &theme,
        |command| draw_in_view(paint, command),
    );
}

fn draw_in_view(paint: &mut PaintCtx<'_>, mut command: DrawCommand<'_>) {
    let rect = paint.rect();
    let parent = paint
        .transform()
        .compose(&Transform::translate(rect.x, rect.y));
    match &mut command {
        DrawCommand::Fill {
            area,
            transform,
            quad,
            ..
        } => {
            *transform = parent.compose(transform);
            if !matches!(
                transform.classify(),
                crate::types::TransformClass::Identity | crate::types::TransformClass::Translate
            ) {
                *quad = Some(transform.apply_rect(*area));
            }
        }
        DrawCommand::FillPath { transform, .. } => {
            *transform = parent.compose(transform);
        }
        _ => unreachable!("Pixel artwork emits fills and contours"),
    }
    paint.draw_clipped(&command, paint.transform().apply_rect_bbox(rect));
}

fn draw_scenery(
    kind: Scenery,
    width: i32,
    height: i32,
    theme: &Theme,
    emit: impl FnMut(DrawCommand<'_>),
) {
    if width <= 0 || height <= 0 {
        return;
    }
    let (mw, mh) = match kind {
        Scenery::Logo => (48, 48),
        Scenery::Landscape | Scenery::Ridge => (112, 40),
        _ => (32, 16),
    };
    let unit = ((mw + width - 1) / width)
        .max((mh + height - 1) / height)
        .max(1);
    Painter {
        theme,
        emit,
        width: width * unit,
        height: height * unit,
        unit,
    }
    .scenery(kind);
}

#[compose]
pub(super) fn backdrop(kind: Scenery) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: Dimension::percent(100),
            height: Dimension::percent(100)
        ) [
            kind,
            crate::ui::IgnoreHitTest,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_scenery_scales_preserve_local_geometry_and_palette_roles() {
        for index in 0..3 {
            let theme = super::super::palette::theme(index);
            for kind in [
                Scenery::Logo,
                Scenery::Header,
                Scenery::Map,
                Scenery::Margins,
                Scenery::Landscape,
                Scenery::Ridge,
            ] {
                for (width, height) in [(16, 16), (48, 48), (320, 240), (640, 480)] {
                    draw_scenery(kind, width, height, &theme, |command| match command {
                        DrawCommand::Fill { area, color, .. } => {
                            assert!(area.w > Fixed::ZERO && area.h > Fixed::ZERO);
                            assert_ne!(color, Color::rgb(255, 0, 255));
                        }
                        DrawCommand::FillPath { .. } => {}
                        _ => panic!("Scenery must emit fills and static contours"),
                    });
                }
            }
        }
    }
}
