use crate::draw::membrane::{MagneticMembrane, MagneticMembraneState};
use crate::draw::canvas::Canvas;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::event::hit_test::hit_test;
use crate::event::input::{InputEvent, KEY_ROTARY_PRESS};
use crate::types::{Color, Fixed, Rect, Viewport};
use crate::widget::{ComputedRect, WidgetRoot};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorFeedbackMode {
    #[default]
    Dot,
    MagneticRect,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CursorVisual {
    pub x: Fixed,
    pub y: Fixed,
    pub down: bool,
    pub target: Option<Entity>,
    pub target_rect: Option<Rect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorFeedback {
    pub enabled: bool,
    pub mode: CursorFeedbackMode,
    pub current: CursorVisual,
    pub prev_bbox: Option<Rect>,
    pub last_event_seq: u32,
    pub seen: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RotaryFeedback {
    pub enabled: bool,
    pub progress: Fixed,
    pub target: Fixed,
    pub velocity: Fixed,
    pub direction: i8,
    pub opacity: Fixed,
    pub prev_bbox: Option<Rect>,
    pub last_input_ms: u32,
    pub pulse: Fixed,
    pub last_input_seq: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputFeedback {
    pub cursor: CursorFeedback,
    pub rotary: RotaryFeedback,
    pub dirty: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct InputFeedbackInput {
    pub rotary_delta: i16,
    pub wheel_delta_y: Fixed,
    pub click_pulse: bool,
    pub event_seq: u32,
}

pub fn record_input(world: &mut World, event: &InputEvent) {
    if world.resource::<InputFeedback>().is_none() {
        return;
    }
    let mut input = world
        .resource::<InputFeedbackInput>()
        .copied()
        .unwrap_or_default();
    match event {
        InputEvent::Rotary { delta, .. } => {
            input.rotary_delta = input.rotary_delta.saturating_add(*delta);
            input.event_seq = input.event_seq.wrapping_add(1);
        }
        InputEvent::Wheel { dy, .. } => {
            input.wheel_delta_y += *dy;
            input.event_seq = input.event_seq.wrapping_add(1);
        }
        InputEvent::Key { code, pressed } if *code == KEY_ROTARY_PRESS && *pressed => {
            input.click_pulse = true;
            input.event_seq = input.event_seq.wrapping_add(1);
        }
        _ => return,
    }
    world.insert_resource(input);
}

fn expand_rect(r: Rect, pad: Fixed) -> Rect {
    Rect {
        x: r.x - pad,
        y: r.y - pad,
        w: r.w + pad * Fixed::from_int(2),
        h: r.h + pad * Fixed::from_int(2),
    }
}

fn cursor_dot_rect(cursor: &CursorVisual) -> Rect {
    let r = Fixed::from_int(if cursor.down { 5 } else { 4 });
    Rect {
        x: cursor.x - r,
        y: cursor.y - r,
        w: r * Fixed::from_int(2),
        h: r * Fixed::from_int(2),
    }
}

fn cursor_bbox(cursor: &CursorFeedback) -> Option<Rect> {
    if !cursor.enabled || !cursor.seen {
        return None;
    }
    match cursor.mode {
        CursorFeedbackMode::Dot => Some(cursor_dot_rect(&cursor.current)),
        CursorFeedbackMode::MagneticRect => Some(cursor.current.target_rect.map_or_else(
            || cursor_dot_rect(&cursor.current),
            |target| expand_rect(target, Fixed::from_int(3)),
        )),
    }
}

fn current_cursor_visual(world: &World, cursor: crate::event::PointerCursor) -> CursorVisual {
    let target = world.resource::<WidgetRoot>().copied().and_then(|root| {
        world
            .resource::<crate::surface::DisplayInfo>()
            .and_then(|info| hit_test(world, root.0, cursor.x, cursor.y, info.width, info.height))
    });
    let target_rect = target.and_then(|e| world.get::<ComputedRect>(e).map(|r| r.0));
    CursorVisual {
        x: cursor.x,
        y: cursor.y,
        down: cursor.down,
        target,
        target_rect,
    }
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

fn rotary_membrane_state(
    rotary: &RotaryFeedback,
    membrane: &MagneticMembrane,
) -> MagneticMembraneState {
    let progress_amp = membrane.max_amp
        * (rotary.progress.abs() / membrane.span().max(Fixed::ONE)).min(Fixed::ONE);
    let pulse_amp = membrane.max_amp * rotary.pulse / Fixed::from_int(2);
    MagneticMembraneState {
        ball_offset: rotary.progress,
        amp: progress_amp.max(pulse_amp),
    }
}

pub fn overlay_dirty_region(world: &World, viewport: &Viewport) -> Option<Rect> {
    let feedback = world.resource::<InputFeedback>()?;
    if !feedback.dirty {
        return None;
    }
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
    feedback.dirty = false;
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
    // ESP rotary-only path never inserts PointerCursor → cursor.current stays (0,0)
    // and cursor_bbox stays None, so a (0,0) ghost dot would never be erased.
    if feedback.cursor.enabled && feedback.cursor.seen {
        match feedback.cursor.mode {
            CursorFeedbackMode::Dot => {
                let area = cursor_dot_rect(&feedback.cursor.current);
                renderer.fill_rect(&area, clip, &primary, area.h / Fixed::from_int(2), 220);
            }
            CursorFeedbackMode::MagneticRect => {
                if let Some(target) = feedback.cursor.current.target_rect {
                    let area = expand_rect(target, Fixed::from_int(2));
                    renderer.fill_rect(&area, clip, &primary, Fixed::from_int(8), 48);
                    renderer.stroke_rect(
                        &area,
                        clip,
                        Fixed::ONE,
                        &primary,
                        Fixed::from_int(8),
                        160,
                    );
                } else {
                    let area = cursor_dot_rect(&feedback.cursor.current);
                    renderer.fill_rect(&area, clip, &primary, area.h / Fixed::from_int(2), 220);
                }
            }
        }
    }
    if feedback.rotary.enabled {
        let Some(_) = rotary_bbox(&feedback.rotary, viewport) else {
            return;
        };
        let (lw, lh) = viewport.logical_size();
        let max_span = Fixed::from(lh) / Fixed::from_int(2) - Fixed::from_int(18);
        let mut membrane = MagneticMembrane::default();
        membrane.visible_span = ((max_span.max(Fixed::ONE) / membrane.sigma) * Fixed::from_int(7)
            / Fixed::from_int(10))
        .max(Fixed::ONE);
        let y_mid = Fixed::from(lh) / Fixed::from_int(2);
        let x = Fixed::from(lw);
        let opa = opa_from_fixed(feedback.rotary.opacity.max(Fixed::ONE / Fixed::from_int(4)));
        let path = membrane.path(x, y_mid, rotary_membrane_state(&feedback.rotary, &membrane));
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
            seen: false,
        }
    }
}

impl Default for RotaryFeedback {
    fn default() -> Self {
        Self {
            enabled: false,
            progress: Fixed::ZERO,
            target: Fixed::ZERO,
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
    let Some(cursor) = world.resource::<crate::event::PointerCursor>().copied() else {
        return;
    };
    let current = current_cursor_visual(world, cursor);
    if feedback.cursor.last_event_seq == cursor.event_seq
        && feedback.cursor.current.x == current.x
        && feedback.cursor.current.y == current.y
        && feedback.cursor.current.down == current.down
        && feedback.cursor.current.target == current.target
        && feedback.cursor.current.target_rect == current.target_rect
    {
        return;
    }
    feedback.cursor.current = current;
    feedback.cursor.last_event_seq = cursor.event_seq;
    feedback.cursor.seen = true;
    feedback.dirty = true;
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
    let before = feedback.rotary;
    let input = world
        .resource::<InputFeedbackInput>()
        .copied()
        .unwrap_or_default();
    if input.event_seq != feedback.rotary.last_input_seq {
        let impulse = Fixed::from_int(input.rotary_delta as i32) - input.wheel_delta_y;
        if impulse != Fixed::ZERO {
            let max_stretch = MagneticMembrane::default().max_pull();
            let next_target = (feedback.rotary.target + impulse * Fixed::from_int(10))
                .clamp(Fixed::ZERO - max_stretch, max_stretch);
            feedback.rotary.velocity += (next_target - feedback.rotary.target) / Fixed::from_int(4);
            feedback.rotary.target = next_target;
            feedback.rotary.direction = if impulse > Fixed::ZERO { 1 } else { -1 };
            feedback.rotary.opacity = Fixed::ONE;
        }
        if input.click_pulse {
            feedback.rotary.pulse = Fixed::ONE;
            feedback.rotary.opacity = Fixed::ONE;
        }
        feedback.rotary.last_input_seq = input.event_seq;
        world.insert_resource(InputFeedbackInput {
            event_seq: input.event_seq,
            ..InputFeedbackInput::default()
        });
    }

    let dt = world
        .resource::<crate::ecs::DeltaTimeMs>()
        .map(|d| d.0.max(1))
        .unwrap_or(16);
    let dt_fixed = Fixed::from_int(dt as i32) / Fixed::from_int(16);
    let spring =
        (feedback.rotary.target - feedback.rotary.progress) * dt_fixed / Fixed::from_int(3);
    feedback.rotary.velocity += spring;
    feedback.rotary.progress += feedback.rotary.velocity * dt_fixed / Fixed::from_int(4);
    feedback.rotary.progress = feedback.rotary.progress.clamp(
        Fixed::ZERO - MagneticMembrane::default().max_pull(),
        MagneticMembrane::default().max_pull(),
    );
    feedback.rotary.velocity = feedback.rotary.velocity * Fixed::from_int(21) / Fixed::from_int(25);
    feedback.rotary.target = feedback.rotary.target * Fixed::from_int(7) / Fixed::from_int(8);
    feedback.rotary.opacity =
        (feedback.rotary.opacity - dt_fixed / Fixed::from_int(25)).max(Fixed::ZERO);
    feedback.rotary.pulse =
        (feedback.rotary.pulse - dt_fixed / Fixed::from_int(16)).max(Fixed::ZERO);
    let settle = Fixed::ONE / Fixed::from_int(128);
    if feedback.rotary.progress.abs() < settle
        && feedback.rotary.velocity.abs() < settle
        && feedback.rotary.target.abs() < settle
    {
        feedback.rotary.progress = Fixed::ZERO;
        feedback.rotary.velocity = Fixed::ZERO;
        feedback.rotary.target = Fixed::ZERO;
    }
    if feedback.rotary != before {
        feedback.dirty = true;
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
    fn cursor_feedback_updates_when_target_rect_moves_without_cursor_event() {
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
        let first = world
            .resource::<InputFeedback>()
            .unwrap()
            .cursor
            .current
            .target_rect;

        world.insert(target, ComputedRect(Rect::new(2, 3, 64, 64)));
        cursor_feedback_system(&mut world);

        let second = world
            .resource::<InputFeedback>()
            .unwrap()
            .cursor
            .current
            .target_rect;
        assert_ne!(first, second);
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
    fn rotary_feedback_consumes_each_input_batch_once() {
        let mut world = World::new();
        world.insert_resource(InputFeedback::enabled());
        world.insert_resource(InputFeedbackInput {
            rotary_delta: 1,
            wheel_delta_y: Fixed::ZERO,
            click_pulse: false,
            event_seq: 1,
        });
        world.insert_resource(crate::ecs::DeltaTimeMs(16));

        rotary_feedback_system(&mut world);
        let first_target = world.resource::<InputFeedback>().unwrap().rotary.target;
        rotary_feedback_system(&mut world);
        let second_target = world.resource::<InputFeedback>().unwrap().rotary.target;

        // target decays *7/8 per frame, so single-consume keeps second_target < first_target.
        // A double-consume would re-add the +10 impulse and push second_target back above first_target.
        let upper = first_target * Fixed::from_int(95) / Fixed::from_int(100);
        assert!(
            second_target < upper,
            "second_target {second_target:?} >= 0.95 * first_target {first_target:?}"
        );
        let input = world.resource::<InputFeedbackInput>().unwrap();
        assert_eq!(input.rotary_delta, 0);
        assert_eq!(input.wheel_delta_y, Fixed::ZERO);
        assert!(!input.click_pulse);
    }

    #[test]
    fn render_overlay_skips_cursor_when_pointer_never_seen() {
        let mut world = World::new();
        let mut feedback = InputFeedback::enabled();
        feedback.rotary.enabled = false;
        debug_assert!(feedback.cursor.enabled);
        debug_assert!(!feedback.cursor.seen);
        world.insert_resource(feedback);

        let mut buf = alloc::vec![0u8; 64 * 64 * 4];
        let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        render_overlay(&world, &viewport, &Rect::new(0, 0, 64, 64), &mut renderer);

        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(
                    renderer.target.get_pixel(x, y).a,
                    0,
                    "ghost dot at ({x},{y})"
                );
            }
        }
    }

    #[test]
    fn rotary_feedback_has_bounded_stretch_under_fast_input() {
        let mut world = World::new();
        world.insert_resource(InputFeedback::enabled());
        world.insert_resource(InputFeedbackInput {
            rotary_delta: 0,
            wheel_delta_y: Fixed::from_int(100),
            click_pulse: false,
            event_seq: 1,
        });
        world.insert_resource(crate::ecs::DeltaTimeMs(16));

        rotary_feedback_system(&mut world);
        let feedback = world.resource::<InputFeedback>().unwrap();
        let max = MagneticMembrane::default().max_pull();
        assert!(feedback.rotary.target.abs() <= max);
        assert!(feedback.rotary.progress.abs() <= max);
        assert!(feedback.rotary.progress < Fixed::ZERO);

        for _ in 0..19 {
            rotary_feedback_system(&mut world);
        }
        let feedback = world.resource::<InputFeedback>().unwrap();
        assert!(feedback.rotary.progress.abs() <= max);
    }

    #[test]
    fn rotary_membrane_state_preserves_progress_sign() {
        let membrane = MagneticMembrane::default();
        let up = RotaryFeedback {
            progress: Fixed::from_int(12),
            direction: 1,
            ..RotaryFeedback::default()
        };
        let down = RotaryFeedback {
            progress: Fixed::from_int(-12),
            direction: -1,
            ..RotaryFeedback::default()
        };

        assert!(rotary_membrane_state(&up, &membrane).ball_offset > Fixed::ZERO);
        assert!(rotary_membrane_state(&down, &membrane).ball_offset < Fixed::ZERO);
    }

    #[test]
    fn rotary_click_pulse_creates_membrane_amplitude() {
        let membrane = MagneticMembrane::default();
        let click = RotaryFeedback {
            pulse: Fixed::ONE,
            ..RotaryFeedback::default()
        };

        assert!(rotary_membrane_state(&click, &membrane).amp > Fixed::ZERO);
    }

    #[test]
    fn overlay_dirty_region_is_none_after_seeding_static_cursor() {
        let mut world = World::new();
        world.insert_resource(InputFeedback::enabled());
        world.insert_resource(crate::event::PointerCursor {
            x: Fixed::from_int(20),
            y: Fixed::from_int(20),
            down: false,
            event_seq: 1,
        });
        cursor_feedback_system(&mut world);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        assert!(overlay_dirty_region(&world, &viewport).is_some());
        seed_overlay_prev_rects(&mut world, &viewport);
        assert!(overlay_dirty_region(&world, &viewport).is_none());
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
        feedback.cursor.seen = true;
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
        let mut water_pixels = 0;
        for y in 0..64 {
            for x in 28..64 {
                if renderer.target.get_pixel(x, y).a != 0 {
                    water_pixels += 1;
                }
            }
        }
        assert!(water_pixels > 0, "expected water-drop pixels");
    }

    #[test]
    fn render_overlay_draws_rotary_click_pulse_without_progress() {
        let mut world = World::new();
        let mut feedback = InputFeedback::enabled();
        feedback.cursor.enabled = false;
        feedback.rotary.pulse = Fixed::ONE;
        feedback.rotary.opacity = Fixed::ONE;
        world.insert_resource(feedback);

        let mut buf = alloc::vec![0u8; 64 * 64 * 4];
        let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        render_overlay(&world, &viewport, &Rect::new(0, 0, 64, 64), &mut renderer);

        let mut pixels = 0;
        for y in 0..64 {
            for x in 28..64 {
                if renderer.target.get_pixel(x, y).a != 0 {
                    pixels += 1;
                }
            }
        }
        assert!(pixels > 0, "expected click pulse pixels");
    }
}

impl InputFeedback {
    pub fn enabled() -> Self {
        Self {
            cursor: CursorFeedback {
                enabled: true,
                mode: CursorFeedbackMode::Dot,
                ..CursorFeedback::default()
            },
            rotary: RotaryFeedback {
                enabled: true,
                ..RotaryFeedback::default()
            },
            dirty: false,
        }
    }
}
