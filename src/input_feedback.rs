use crate::draw::canvas::Canvas;
use crate::draw::path::Path;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::event::hit_test::hit_test;
use crate::types::{Color, Fixed, Point, Rect, Viewport};
use crate::widget::{ComputedRect, WidgetRoot};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorFeedbackMode {
    #[default]
    Dot,
    MagneticRect,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CursorVisual {
    pub x: Fixed,
    pub y: Fixed,
    pub down: bool,
    pub target: Option<Entity>,
    pub target_rect: Option<Rect>,
}

#[derive(Clone, Copy, Debug)]
pub struct CursorFeedback {
    pub enabled: bool,
    pub mode: CursorFeedbackMode,
    pub current: CursorVisual,
    pub prev_bbox: Option<Rect>,
    pub last_event_seq: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct RotaryFeedback {
    pub enabled: bool,
    pub progress: Fixed,
    pub velocity: Fixed,
    pub direction: i8,
    pub opacity: Fixed,
    pub prev_bbox: Option<Rect>,
    pub last_input_ms: u32,
    pub pulse: Fixed,
    pub last_input_seq: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct InputFeedback {
    pub cursor: CursorFeedback,
    pub rotary: RotaryFeedback,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct InputFeedbackInput {
    pub rotary_delta: i16,
    pub wheel_delta_y: Fixed,
    pub click_pulse: bool,
    pub event_seq: u32,
}

fn expand_rect(r: Rect, pad: Fixed) -> Rect {
    Rect {
        x: r.x - pad,
        y: r.y - pad,
        w: r.w + pad * Fixed::from_int(2),
        h: r.h + pad * Fixed::from_int(2),
    }
}

fn cursor_bbox(cursor: &CursorFeedback) -> Option<Rect> {
    if !cursor.enabled {
        return None;
    }
    cursor.current.target_rect.map_or_else(
        || {
            let r = Fixed::from_int(if cursor.current.down { 5 } else { 4 });
            Some(Rect {
                x: cursor.current.x - r,
                y: cursor.current.y - r,
                w: r * Fixed::from_int(2),
                h: r * Fixed::from_int(2),
            })
        },
        |target| Some(expand_rect(target, Fixed::from_int(3))),
    )
}

fn rotary_bbox(rotary: &RotaryFeedback, viewport: &Viewport) -> Option<Rect> {
    if !rotary.enabled {
        return None;
    }
    if rotary.progress == Fixed::ZERO
        && rotary.opacity == Fixed::ZERO
        && rotary.pulse == Fixed::ZERO
    {
        return None;
    }
    let (lw, lh) = viewport.logical_size();
    Some(Rect::new(
        Fixed::from(lw) - Fixed::from_int(36),
        Fixed::ZERO,
        Fixed::from_int(36),
        Fixed::from(lh),
    ))
}

fn overlay_bbox(feedback: &InputFeedback, viewport: &Viewport) -> Option<Rect> {
    let mut out = cursor_bbox(&feedback.cursor);
    if let Some(r) = rotary_bbox(&feedback.rotary, viewport) {
        out = Some(out.map_or(r, |o| o.union(&r)));
    }
    out
}

fn opa_from_fixed(v: Fixed) -> u8 {
    let raw = (v.clamp(Fixed::ZERO, Fixed::ONE) * Fixed::from_int(255)).to_int();
    raw.clamp(0, 255) as u8
}

fn cubic(t: Fixed, x0: Fixed, x1: Fixed, x2: Fixed, x3: Fixed) -> Fixed {
    let one_t = Fixed::ONE - t;
    one_t * one_t * one_t * x0
        + Fixed::from_int(3) * one_t * one_t * t * x1
        + Fixed::from_int(3) * one_t * t * t * x2
        + t * t * t * x3
}

fn water_drop_path(edge_x: Fixed, mid_y: Fixed, pull: Fixed, dir: i8) -> Path {
    let w = Fixed::from_int(34);
    let h = Fixed::from_int(88);
    let flatten = Fixed::from_int(22);
    let sharpen = Fixed::from_int(18);
    let progress = pull.abs().clamp(Fixed::from_int(2), w);
    let tall_ratio = progress / w;
    let tall = progress;
    let lean = Fixed::from_int(dir as i32);
    let top = mid_y - h / Fixed::from_int(2);
    let attach_top = Point { x: edge_x, y: top };
    let attach_bottom = Point {
        x: edge_x,
        y: top + h,
    };
    let mut path = Path::new();
    path.move_to(attach_top);
    for i in 0..=16 {
        let t = Fixed::from_raw(i * Fixed::ONE.raw() / 16);
        let row = cubic(
            t,
            Fixed::ZERO,
            (Fixed::ONE + tall_ratio) * flatten,
            h / Fixed::from_int(2) - flatten + sharpen * lean + tall_ratio * sharpen,
            h / Fixed::from_int(2) + sharpen * lean,
        );
        let col = cubic(t, Fixed::ZERO, Fixed::ZERO, tall, tall);
        path.line_to(Point {
            x: edge_x - col,
            y: top + row,
        });
    }
    let rest = h / Fixed::from_int(2) + sharpen * lean;
    for i in 0..=16 {
        let t = Fixed::from_raw(i * Fixed::ONE.raw() / 16);
        let row = cubic(
            t,
            rest,
            h / Fixed::from_int(2) + flatten + sharpen * lean - tall_ratio * sharpen,
            h - (Fixed::ONE + tall_ratio) * flatten,
            h,
        );
        let col = cubic(t, tall, tall, Fixed::ZERO, Fixed::ZERO);
        path.line_to(Point {
            x: edge_x - col,
            y: top + row,
        });
    }
    path.line_to(attach_bottom).close();
    path
}

pub fn overlay_dirty_region(world: &World, viewport: &Viewport) -> Option<Rect> {
    let feedback = world.resource::<InputFeedback>()?;
    let curr = overlay_bbox(feedback, viewport);
    let mut out = curr;
    if let Some(prev) = feedback.cursor.prev_bbox {
        out = Some(out.map_or(prev, |o| o.union(&prev)));
    }
    if let Some(prev) = feedback.rotary.prev_bbox {
        out = Some(out.map_or(prev, |o| o.union(&prev)));
    }
    out
}

pub fn seed_overlay_prev_rects(world: &mut World, viewport: &Viewport) {
    let Some(mut feedback) = world.resource::<InputFeedback>().copied() else {
        return;
    };
    feedback.cursor.prev_bbox = cursor_bbox(&feedback.cursor);
    feedback.rotary.prev_bbox = rotary_bbox(&feedback.rotary, viewport);
    world.insert_resource(feedback);
}

pub fn render_overlay(
    world: &World,
    viewport: &Viewport,
    clip: &Rect,
    renderer: &mut (impl Renderer + Canvas),
) {
    let Some(feedback) = world.resource::<InputFeedback>() else {
        return;
    };
    let primary = Color::rgb(88, 166, 255);
    if feedback.cursor.enabled {
        if let Some(target) = feedback.cursor.current.target_rect {
            let area = expand_rect(target, Fixed::from_int(2));
            renderer.fill_rect(&area, clip, &primary, Fixed::from_int(8), 48);
            renderer.stroke_rect(&area, clip, Fixed::ONE, &primary, Fixed::from_int(8), 160);
        } else {
            let r = Fixed::from_int(if feedback.cursor.current.down { 5 } else { 4 });
            let area = Rect {
                x: feedback.cursor.current.x - r,
                y: feedback.cursor.current.y - r,
                w: r * Fixed::from_int(2),
                h: r * Fixed::from_int(2),
            };
            renderer.fill_rect(&area, clip, &primary, r, 220);
        }
    }
    if feedback.rotary.enabled {
        let Some(_) = rotary_bbox(&feedback.rotary, viewport) else {
            return;
        };
        let (lw, lh) = viewport.logical_size();
        let stretch = feedback.rotary.progress.abs().min(Fixed::from_int(80));
        let y_mid = Fixed::from(lh) / Fixed::from_int(2)
            - Fixed::from(feedback.rotary.direction as i32) * stretch / Fixed::from_int(3);
        let x = Fixed::from(lw) - Fixed::from_int(16);
        let opa = opa_from_fixed(feedback.rotary.opacity.max(Fixed::from_raw(64)));
        let path = water_drop_path(x, y_mid, stretch, feedback.rotary.direction);
        renderer.fill_path(&path, clip, &primary, opa);
    }
}

impl Default for CursorFeedback {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: CursorFeedbackMode::Dot,
            current: CursorVisual::default(),
            prev_bbox: None,
            last_event_seq: 0,
        }
    }
}

impl Default for RotaryFeedback {
    fn default() -> Self {
        Self {
            enabled: false,
            progress: Fixed::ZERO,
            velocity: Fixed::ZERO,
            direction: 0,
            opacity: Fixed::ZERO,
            prev_bbox: None,
            last_input_ms: 0,
            pulse: Fixed::ZERO,
            last_input_seq: 0,
        }
    }
}

#[crate::system(order = NORMAL)]
pub fn cursor_feedback_system(world: &mut World) {
    let Some(mut feedback) = world.resource::<InputFeedback>().copied() else {
        return;
    };
    if !feedback.cursor.enabled {
        return;
    }
    let cursor = world
        .resource::<crate::event::PointerCursor>()
        .copied()
        .unwrap_or_default();
    if feedback.cursor.last_event_seq == cursor.event_seq
        && feedback.cursor.current.x == cursor.x
        && feedback.cursor.current.y == cursor.y
        && feedback.cursor.current.down == cursor.down
    {
        return;
    }
    let target = world.resource::<WidgetRoot>().copied().and_then(|root| {
        world
            .resource::<crate::surface::DisplayInfo>()
            .and_then(|info| hit_test(world, root.0, cursor.x, cursor.y, info.width, info.height))
    });
    let target_rect = target.and_then(|e| world.get::<ComputedRect>(e).map(|r| r.0));
    feedback.cursor.current = CursorVisual {
        x: cursor.x,
        y: cursor.y,
        down: cursor.down,
        target,
        target_rect,
    };
    feedback.cursor.last_event_seq = cursor.event_seq;
    world.insert_resource(feedback);
}

#[crate::system(order = NORMAL)]
pub fn rotary_feedback_system(world: &mut World) {
    let Some(mut feedback) = world.resource::<InputFeedback>().copied() else {
        return;
    };
    if !feedback.rotary.enabled {
        return;
    }
    let input = world
        .resource::<InputFeedbackInput>()
        .copied()
        .unwrap_or_default();
    if input.event_seq != feedback.rotary.last_input_seq {
        let impulse = Fixed::from_int(input.rotary_delta as i32) + input.wheel_delta_y;
        if impulse != Fixed::ZERO {
            feedback.rotary.velocity += impulse * Fixed::from_int(4);
            feedback.rotary.direction = if impulse > Fixed::ZERO { 1 } else { -1 };
            feedback.rotary.opacity = Fixed::ONE;
        }
        if input.click_pulse {
            feedback.rotary.pulse = Fixed::ONE;
            feedback.rotary.opacity = Fixed::ONE;
        }
        feedback.rotary.last_input_seq = input.event_seq;
    }

    let dt = world
        .resource::<crate::ecs::DeltaTimeMs>()
        .map(|d| d.0.max(1))
        .unwrap_or(16);
    let dt_fixed = Fixed::from_int(dt as i32) / Fixed::from_int(16);
    feedback.rotary.progress += feedback.rotary.velocity * dt_fixed / Fixed::from_int(8);
    feedback.rotary.velocity -= feedback.rotary.progress * dt_fixed / Fixed::from_int(3);
    feedback.rotary.velocity = feedback.rotary.velocity * Fixed::from_raw(220);
    feedback.rotary.opacity =
        (feedback.rotary.opacity - Fixed::from_raw(10) * dt_fixed).max(Fixed::ZERO);
    feedback.rotary.pulse =
        (feedback.rotary.pulse - Fixed::from_raw(16) * dt_fixed).max(Fixed::ZERO);
    if feedback.rotary.progress.abs() < Fixed::from_raw(2)
        && feedback.rotary.velocity.abs() < Fixed::from_raw(2)
    {
        feedback.rotary.progress = Fixed::ZERO;
        feedback.rotary.velocity = Fixed::ZERO;
    }
    world.insert_resource(feedback);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::sw::SwRenderer;
    use crate::draw::texture::ColorFormat;
    use crate::draw::texture::Texture;
    use crate::event::PointerCursor;
    use crate::layout::LayoutStyle;
    use crate::surface::DisplayInfo;
    use crate::types::Dimension;
    use crate::widget::{Children, Parent, Style, Widget};

    fn spawn_widget(world: &mut World, parent: Option<Entity>, style: Style) -> Entity {
        let e = world.spawn();
        world.insert(e, Widget);
        world.insert(e, style);
        if let Some(p) = parent {
            world.insert(e, Parent(p));
            if let Some(children) = world.get_mut::<Children>(p) {
                children.0.push(e);
            } else {
                world.insert(p, Children(alloc::vec![e]));
            }
        }
        e
    }

    #[test]
    fn cursor_feedback_tracks_hover_target_rect() {
        let mut world = World::new();
        world.insert_resource(DisplayInfo {
            width: 128,
            height: 128,
            scale: Fixed::ONE,
            format: ColorFormat::RGBA8888,
        });
        world.insert_resource(InputFeedback::enabled());
        let root = spawn_widget(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(128),
                    height: Dimension::px(128),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let target = spawn_widget(
            &mut world,
            Some(root),
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(64),
                    height: Dimension::px(64),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert_resource(WidgetRoot(root));
        crate::widget::render_system::update_layout(
            &mut world,
            root,
            &crate::types::Viewport::new(128, 128, Fixed::ONE),
        );
        world.insert_resource(PointerCursor {
            x: Fixed::from_int(10),
            y: Fixed::from_int(10),
            down: false,
            event_seq: 1,
        });

        cursor_feedback_system(&mut world);

        let feedback = world.resource::<InputFeedback>().unwrap();
        assert_eq!(feedback.cursor.current.target, Some(target));
        assert!(feedback.cursor.current.target_rect.is_some());
    }

    #[test]
    fn rotary_feedback_consumes_rotary_and_wheel_impulses() {
        let mut world = World::new();
        world.insert_resource(InputFeedback::enabled());
        world.insert_resource(InputFeedbackInput {
            rotary_delta: 2,
            wheel_delta_y: Fixed::from_int(1),
            click_pulse: true,
            event_seq: 1,
        });
        world.insert_resource(crate::ecs::DeltaTimeMs(16));

        rotary_feedback_system(&mut world);

        let feedback = world.resource::<InputFeedback>().unwrap();
        assert!(feedback.rotary.progress != Fixed::ZERO || feedback.rotary.velocity != Fixed::ZERO);
        assert_eq!(feedback.rotary.direction, 1);
        assert!(feedback.rotary.opacity > Fixed::ZERO);
        assert!(feedback.rotary.pulse > Fixed::ZERO);
    }

    #[test]
    fn render_overlay_draws_cursor_and_rotary_feedback() {
        let mut world = World::new();
        let mut feedback = InputFeedback::enabled();
        feedback.cursor.current = CursorVisual {
            x: Fixed::from_int(20),
            y: Fixed::from_int(20),
            down: false,
            target: None,
            target_rect: None,
        };
        feedback.rotary.progress = Fixed::from_int(30);
        feedback.rotary.direction = 1;
        feedback.rotary.opacity = Fixed::ONE;
        world.insert_resource(feedback);

        let mut buf = alloc::vec![0u8; 64 * 64 * 4];
        let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        render_overlay(&world, &viewport, &Rect::new(0, 0, 64, 64), &mut renderer);

        assert_ne!(renderer.target.get_pixel(20, 20).a, 0);
        assert_ne!(renderer.target.get_pixel(30, 24).a, 0);
    }
}

impl InputFeedback {
    pub fn enabled() -> Self {
        Self {
            cursor: CursorFeedback {
                enabled: true,
                mode: CursorFeedbackMode::MagneticRect,
                ..CursorFeedback::default()
            },
            rotary: RotaryFeedback {
                enabled: true,
                ..RotaryFeedback::default()
            },
        }
    }
}
