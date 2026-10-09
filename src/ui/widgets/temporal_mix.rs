//! Temporal IIR mix of `source`. Each successful output is `α * prev_output + (1 - α) * source_curr`, where α = `mix / 255`. A source color change decays across successive outputs.
//!
//! `mix=0` shows only the current source; `mix=255` freezes the initial output.
//!
//! The widget retains its own previous successful output. Its history stays fixed throughout one drawing batch, including multiple dirty regions.

use crate::ecs::{Entity, World};
use crate::render::PlaneLayout;
use crate::render::command::{CompositeMode, DrawCommand};
use crate::render::texture::{AlphaMode, ColorFormat, Texture};
use crate::types::{Fixed, Point, Rect};
use crate::ui::offscreen::WidgetTexture;
use crate::ui::outputs::Outputs;
use crate::ui::view::{PaintMeasurement, SourceGeometry, View};

#[crate::component]
#[retain_previous(WidgetTexture)]
pub struct TemporalMix {
    #[depends_on(WidgetTexture)]
    pub source: Entity,
    pub mix: u8,
}

impl TemporalMix {
    pub fn new(source: Entity) -> Self {
        Self { source, mix: 128 }
    }

    pub fn with_mix(mut self, mix: u8) -> Self {
        self.mix = mix;
        self
    }
}

#[crate::view(component = TemporalMix, name = "TemporalMix", priority = 60)]
fn temporal_mix_render(
    temporal: component!(),
    entity: entity!(),
    outputs: res!(Outputs<WidgetTexture>),
    paint: paint!(),
) {
    let Some(current) = outputs.get(temporal.source) else {
        return;
    };
    let curr = current.texture();
    if curr.format != ColorFormat::RGBA8888 {
        return;
    }
    let Ok(layout) = PlaneLayout::packed(curr.width, curr.height, curr.format) else {
        return;
    };
    let previous = outputs.previous(entity);
    let previous = previous.as_ref().map(|output| output.texture());
    let rect = paint.rect();
    paint.with_workspace(layout, |texture, paint| {
        texture.alpha_mode = AlphaMode::Blend;
        texture.transient = true;
        let unfinished = mix_into(texture, curr, previous, temporal.mix);
        paint.draw(&DrawCommand::Blit {
            pos: Point::new(rect.x, rect.y),
            size: Point::new(rect.w, rect.h),
            transform: paint.transform(),
            quad: paint.quad(),
            texture,
            opa: 255,
            radius: Fixed::ZERO,
            composite: CompositeMode::SourceOver,
        });
        if unfinished {
            paint.request_frame_on_change();
        }
    });
}

fn temporal_measurement(
    world: &World,
    entity: Entity,
    _rect: Rect,
    source_geometry: &dyn Fn(Entity) -> Option<SourceGeometry>,
) -> Option<PaintMeasurement> {
    let temporal = world.get::<TemporalMix>(entity)?;
    let source = source_geometry(temporal.source)?.plane;
    (source.format() == ColorFormat::RGBA8888).then_some(PaintMeasurement {
        bounds: None,
        workspace: Some(source),
    })
}

fn mix_into(dst: &mut Texture, source: &Texture, previous: Option<&Texture>, mix: u8) -> bool {
    let previous = previous.filter(|previous| {
        previous.width == source.width
            && previous.height == source.height
            && previous.format == source.format
    });
    let row_bytes = usize::from(source.width) * 4;
    let dst_stride = dst.stride;
    let dst = dst.buf.as_mut_slice();
    let source_buf = source.buf.as_slice();
    let mut unfinished = false;
    for y in 0..usize::from(source.height) {
        let target = &source_buf[y * source.stride..][..row_bytes];
        let row = &mut dst[y * dst_stride..][..row_bytes];
        match previous {
            Some(previous) if mix != 0 => {
                let old = &previous.buf.as_slice()[y * previous.stride..][..row_bytes];
                if mix == 255 {
                    row.copy_from_slice(old);
                    continue;
                }
                for ((out, &target), &old) in row.iter_mut().zip(target).zip(old) {
                    let mut value = ((u16::from(255 - mix) * u16::from(target)
                        + u16::from(mix) * u16::from(old))
                        / 255) as u8;
                    if value == old && old != target {
                        value = if target > old { old + 1 } else { old - 1 };
                    }
                    unfinished |= value != target;
                    *out = value;
                }
            }
            _ => row.copy_from_slice(target),
        }
    }
    unfinished
}

pub fn view() -> View {
    temporal_mix_render::view().with_measurement(temporal_measurement)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;
    use crate::surface::FramebufferAccess;

    type TestApp =
        App<crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>>;

    const RED: Color = Color::rgb(255, 0, 0);
    const BLUE: Color = Color::rgb(0, 0, 255);
    const PLANE_BYTES: usize = 8 * 8 * 4;
    const BUDGET: usize = 4 * PLANE_BYTES;

    #[crate::compose]
    fn temporal_fixture(mix: u8) {
        ui! {
            Row (width: 16, height: 8) {
                View (id: "temporal_source", width: 8, height: 8, bg_color: Color::rgb(255, 0, 0))
                View (id: "temporal_output", width: 8, height: 8) [
                    TemporalMix::new(id("temporal_source")).with_mix(mix),
                ]
            }
        };
    }

    fn test_app(mix: u8, budget: usize) -> (TestApp, Entity, Entity) {
        let mut app = App::headless(16, 8);
        app.with_default_widgets()
            .with_offscreen_pool_budget(budget);
        let root = app.spawn_root().bg_color(Color::rgb(0, 0, 0)).id();
        app.compose(root, temporal_fixture(mix));
        let source = app.world.find_by_id("temporal_source").unwrap();
        let owner = app.world.find_by_id("temporal_output").unwrap();
        (app, source, owner)
    }

    fn output(app: &TestApp, entity: Entity) -> (Color, u64) {
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        let output = outputs.get(entity).unwrap();
        (
            output.texture().get_pixel(1, 1),
            output.texture().cache_revision,
        )
    }

    fn set_color(app: &mut TestApp, entity: Entity, color: Color) {
        app.world.get_mut::<Style>(entity).unwrap().bg_color = Some(color.into());
    }

    fn assert_idle(app: &mut TestApp, owner: Entity) {
        let settled = output(app, owner);
        for _ in 0..3 {
            app.render_dirty().unwrap();
            assert_eq!(output(app, owner), settled);
            assert!(
                app.world
                    .resource::<crate::ui::render_system::LastDirtyRegions>()
                    .unwrap()
                    .0
                    .is_empty()
            );
        }
    }

    fn converge(app: &mut TestApp, owner: Entity, target: Color) -> usize {
        for frame in 1..=255 {
            let before = output(app, owner);
            app.render_dirty().unwrap();
            let after = output(app, owner);
            assert!(after.1 > before.1);
            for (old, new, goal) in [
                (before.0.r, after.0.r, target.r),
                (before.0.g, after.0.g, target.g),
                (before.0.b, after.0.b, target.b),
                (before.0.a, after.0.a, target.a),
            ] {
                if old < goal {
                    assert!(old < new && new <= goal);
                } else if old > goal {
                    assert!(goal <= new && new < old);
                } else {
                    assert_eq!(new, goal);
                }
            }
            if after.0 == target {
                return frame;
            }
        }
        panic!("Temporal output did not converge in 255 successful batches");
    }

    #[test]
    fn temporal_mix_retains_its_own_completed_output_between_batches() {
        let mut app = App::headless(16, 8);
        app.with_default_widgets()
            .with_offscreen_pool_budget(2 * 1024);
        let root = app.spawn_root().id();
        app.compose(root, temporal_fixture(128));
        let source = app.world.find_by_id("temporal_source").unwrap();
        let owner = app.world.find_by_id("temporal_output").unwrap();
        app.render().unwrap();
        {
            let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
            assert_eq!(
                outputs.get(owner).unwrap().texture().get_pixel(1, 1),
                Color::rgb(255, 0, 0)
            );
            assert!(outputs.previous(owner).is_none());
        }

        app.world.get_mut::<Style>(source).unwrap().bg_color = Some(Color::rgb(0, 0, 255).into());
        app.world.invalidate(source);
        app.world.invalidate(owner);
        app.render_dirty().unwrap();
        let mixed = {
            let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
            assert!(outputs.previous(source).is_none());
            assert_eq!(
                outputs.previous(owner).unwrap().texture().get_pixel(1, 1),
                Color::rgb(255, 0, 0)
            );
            let pixel = outputs.get(owner).unwrap().texture().get_pixel(1, 1);
            assert!((126..=129).contains(&pixel.r));
            assert_eq!(pixel.g, 0);
            assert!((126..=129).contains(&pixel.b));
            pixel
        };

        app.world.invalidate(owner);
        app.render_dirty().unwrap();
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        assert_eq!(
            outputs.previous(owner).unwrap().texture().get_pixel(1, 1),
            mixed
        );
        let decayed = outputs.get(owner).unwrap().texture().get_pixel(1, 1);
        assert!(decayed.r < mixed.r);
        assert!(decayed.b > mixed.b);
    }

    #[test]
    fn full_render_republishes_changed_source_and_retains_owner_history() {
        let mut app = App::headless(16, 8);
        app.with_default_widgets()
            .with_offscreen_pool_budget(2 * 1024);
        let root = app.spawn_root().id();
        app.compose(root, temporal_fixture(128));
        let source = app.world.find_by_id("temporal_source").unwrap();
        let owner = app.world.find_by_id("temporal_output").unwrap();
        app.render().unwrap();

        app.world.get_mut::<Style>(source).unwrap().bg_color = Some(Color::rgb(0, 0, 255).into());
        app.world.invalidate(source);
        app.world.invalidate(owner);
        app.render().unwrap();

        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        assert_eq!(
            outputs.get(source).unwrap().texture().get_pixel(1, 1),
            Color::rgb(0, 0, 255)
        );
        assert_eq!(
            outputs.previous(owner).unwrap().texture().get_pixel(1, 1),
            Color::rgb(255, 0, 0)
        );
        let mixed = outputs.get(owner).unwrap().texture().get_pixel(1, 1);
        assert!((126..=129).contains(&mixed.r));
        assert!((126..=129).contains(&mixed.b));
    }

    #[test]
    fn source_change_converges_in_finite_app_batches_without_manual_invalidation() {
        for mix in [1, 128, 254] {
            let (mut app, source, owner) = test_app(mix, BUDGET);
            app.render().unwrap();
            assert_eq!(output(&app, owner).0, RED);
            assert_idle(&mut app, owner);

            set_color(&mut app, source, BLUE);
            let frames = converge(&mut app, owner, BLUE);
            if mix == 254 {
                assert_eq!(frames, 255);
            }
            assert_eq!(app.backend.framebuffer().get_pixel(9, 1), BLUE);
            let source_version = output(&app, source).1;
            assert_idle(&mut app, owner);
            assert_eq!(output(&app, source).1, source_version);
        }
    }

    #[test]
    fn temporal_alpha_converges_without_treating_borrowed_pixels_as_opaque() {
        let (mut app, source, owner) = test_app(254, BUDGET);
        let faint = Color::rgba(255, 0, 0, 1);
        set_color(&mut app, source, faint);
        app.render().unwrap();
        assert_eq!(output(&app, source).0, faint);
        assert_eq!(output(&app, owner).0, faint);
        assert_idle(&mut app, owner);

        set_color(&mut app, source, RED);
        assert_eq!(converge(&mut app, owner, RED), 254);
        assert_idle(&mut app, owner);
    }

    #[test]
    fn opaque_temporal_background_stops_at_the_final_composited_fixed_point() {
        let (mut app, source, owner) = test_app(128, BUDGET);
        set_color(&mut app, owner, Color::rgb(0, 0, 0));
        app.render().unwrap();
        set_color(&mut app, source, Color::rgba(255, 0, 0, 128));

        let mut previous = output(&app, owner).0;
        let mut settled = false;
        for _ in 0..32 {
            app.render_dirty().unwrap();
            let current = output(&app, owner).0;
            if current == previous {
                settled = true;
                break;
            }
            previous = current;
        }
        assert!(
            settled,
            "The final composited pixels must reach a fixed point"
        );
        assert_eq!(previous.a, 255);
        assert!(previous.r > 128 && previous.r < 255);
        assert_idle(&mut app, owner);
    }

    #[crate::compose]
    fn temporal_cover(width: i32) {
        ui! {
            View (
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: width,
                height: 8,
                bg_color: Color::rgb(0, 255, 0)
            )
        };
    }

    #[test]
    fn temporal_children_stop_covered_pixels_without_stopping_visible_convergence() {
        for cover_width in [4, 8] {
            let (mut app, source, owner) = test_app(128, BUDGET);
            app.compose(owner, temporal_cover(cover_width));
            app.render().unwrap();
            let mut version = output(&app, owner).1;
            set_color(&mut app, source, BLUE);
            let mut stopped = false;
            let mut produced = 0;
            for _ in 0..32 {
                app.render_dirty().unwrap();
                let (covered, current_version) = output(&app, owner);
                assert_eq!(covered, Color::rgb(0, 255, 0));
                if current_version == version {
                    stopped = true;
                    break;
                }
                version = current_version;
                produced += 1;
            }
            assert!(
                stopped,
                "Child composition must not cause permanent repainting"
            );
            let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
            let texture = outputs.get(owner).unwrap();
            let right = texture.texture().get_pixel(5, 1);
            if cover_width == 4 {
                assert_eq!(right, BLUE);
                assert!(produced > 1);
            } else {
                assert_eq!(right, Color::rgb(0, 255, 0));
                assert_eq!(produced, 1);
            }
            drop(texture);
            assert_idle(&mut app, owner);
        }
    }

    #[test]
    fn first_output_and_endpoint_mix_values_do_not_request_more_frames() {
        for mix in [0, 128, 255] {
            let (mut app, source, owner) = test_app(mix, BUDGET);
            app.render().unwrap();
            assert_eq!(output(&app, owner).0, RED);
            assert!(
                app.world
                    .resource::<Outputs<WidgetTexture>>()
                    .unwrap()
                    .previous(owner)
                    .is_none()
            );
            assert_idle(&mut app, owner);
            if mix == 128 {
                continue;
            }
            set_color(&mut app, source, BLUE);
            app.render_dirty().unwrap();
            assert_eq!(output(&app, source).0, BLUE);
            assert_eq!(output(&app, owner).0, if mix == 0 { BLUE } else { RED });
            assert_idle(&mut app, owner);
        }
    }

    #[test]
    fn incompatible_history_uses_source_without_requesting_more_frames() {
        let (mut app, source, owner) = test_app(128, BUDGET);
        app.render().unwrap();
        app.world
            .insert(source, crate::ui::OffscreenRender::with_scale(Fixed::HALF));
        set_color(&mut app, source, BLUE);
        app.render_dirty().unwrap();
        {
            let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
            assert_eq!(outputs.get(source).unwrap().texture().width, 4);
            assert_eq!(outputs.get(owner).unwrap().texture().width, 8);
        }
        assert_eq!(output(&app, owner).0, BLUE);
        assert_idle(&mut app, owner);
    }

    #[crate::compose]
    fn replacement_source() {
        ui! {
            View (
                id: "replacement_source",
                position: Position::Absolute,
                width: 8,
                height: 8,
                bg_color: BLUE
            )
        };
    }

    #[test]
    fn replacing_the_source_clears_frozen_temporal_history() {
        let (mut app, _, owner) = test_app(255, BUDGET);
        app.render().unwrap();
        let root = app.root.unwrap();
        app.compose(root, replacement_source());
        let replacement = app.world.find_by_id("replacement_source").unwrap();
        app.world.get_mut::<TemporalMix>(owner).unwrap().source = replacement;
        app.render_dirty().unwrap();
        assert_eq!(output(&app, owner).0, BLUE);
        assert!(
            app.world
                .resource::<Outputs<WidgetTexture>>()
                .unwrap()
                .previous(owner)
                .is_none()
        );
        assert_idle(&mut app, owner);
    }

    #[test]
    fn unavailable_source_or_workspace_does_not_keep_the_app_drawing() {
        for budget in [BUDGET, 3 * PLANE_BYTES - 1] {
            let (mut app, source, owner) = test_app(128, budget);
            if budget == BUDGET {
                app.world.insert(source, crate::ui::Hidden);
            }
            app.render().unwrap();
            {
                let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
                if budget == BUDGET {
                    assert!(outputs.get(source).is_none());
                    assert_eq!(outputs.get(owner).unwrap().texture().get_pixel(1, 1).a, 0);
                } else {
                    assert!(outputs.get(source).is_some());
                    assert!(outputs.get(owner).is_none());
                }
            }
            for _ in 0..3 {
                app.render_dirty().unwrap();
                assert!(
                    app.world
                        .resource::<crate::ui::render_system::LastDirtyRegions>()
                        .unwrap()
                        .0
                        .is_empty()
                );
            }
        }
    }

    #[test]
    fn unavailable_second_history_slot_does_not_request_more_frames() {
        let (mut app, source, owner) = test_app(128, BUDGET - 1);
        app.render().unwrap();
        assert_eq!(output(&app, owner).0, RED);
        assert_idle(&mut app, owner);
        set_color(&mut app, source, BLUE);
        app.render_dirty().unwrap();
        {
            let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
            assert!(outputs.get(owner).is_none());
            assert_eq!(
                outputs.previous(owner).unwrap().texture().get_pixel(1, 1),
                RED
            );
            assert_eq!(outputs.get(source).unwrap().texture().get_pixel(1, 1), BLUE);
        }
        for _ in 0..3 {
            app.render_dirty().unwrap();
            assert!(
                app.world
                    .resource::<Outputs<WidgetTexture>>()
                    .unwrap()
                    .get(owner)
                    .is_none()
            );
            assert!(
                app.world
                    .resource::<crate::ui::render_system::LastDirtyRegions>()
                    .unwrap()
                    .0
                    .is_empty()
            );
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn warm_temporal_convergence_reuses_history_workspace_and_frame_requests() {
        let (mut app, source, owner) = test_app(128, BUDGET);
        app.render().unwrap();
        for color in [BLUE, RED, BLUE, RED] {
            set_color(&mut app, source, color);
            converge(&mut app, owner, color);
            assert_idle(&mut app, owner);
        }
        let allocations = crate::test_support::tracked_allocations(|| {
            for frame in 0..32 {
                let target = if frame % 2 == 0 { BLUE } else { RED };
                set_color(&mut app, source, target);
                assert!(converge(&mut app, owner, target) > 1);
                assert_idle(&mut app, owner);
            }
        });
        assert_eq!(allocations, 0);
    }

    #[test]
    fn temporal_workspace_respects_each_borrowed_row_stride_and_alpha_channel() {
        let mut source_bytes = [0xee; 24];
        let mut previous_bytes = [0xdd; 32];
        for y in 0..2 {
            for x in 0..2 {
                source_bytes[y * 12 + x * 4..][..4].copy_from_slice(&[2, 1, 8, 32]);
                previous_bytes[y * 16 + x * 4..][..4].copy_from_slice(&[1, 240, 8, 31]);
            }
        }
        let mut source = Texture::from_ref(&source_bytes, 2, 2, ColorFormat::RGBA8888);
        source.stride = 12;
        let mut previous = Texture::from_ref(&previous_bytes, 2, 2, ColorFormat::RGBA8888);
        previous.stride = 16;
        for (mix, expected, unfinished) in [
            (0, [2, 1, 8, 32], false),
            (254, [2, 239, 8, 32], true),
            (255, [1, 240, 8, 31], false),
        ] {
            let mut bytes = [0xcc; 40];
            let mut dst = Texture::new(&mut bytes, 2, 2, ColorFormat::RGBA8888);
            dst.stride = 20;
            assert_eq!(
                mix_into(&mut dst, &source, Some(&previous), mix),
                unfinished
            );
            let pixels = dst.buf.as_slice();
            for y in 0..2 {
                for x in 0..2 {
                    assert_eq!(&pixels[y * 20 + x * 4..][..4], &expected);
                }
                assert_eq!(&pixels[y * 20 + 8..][..12], &[0xcc; 12]);
            }
        }
    }
}
