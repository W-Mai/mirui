//! Temporal IIR mix of `source`. Each successful output is `α * prev_output + (1 - α) * source_curr`, where α = `mix / 255`. A source color change decays across successive outputs.
//!
//! `mix=0` shows only the current source; `mix=255` freezes the initial output.
//!
//! The widget retains its own previous successful output. Its history stays fixed throughout one drawing batch, including multiple dirty regions.

use crate::ecs::{Entity, World};
use crate::render::command::{CompositeMode, DrawCommand};
use crate::render::sw::mix::mix_inplace;
use crate::render::texture::{ColorFormat, Texture};
use crate::types::{Fixed, Point};
use crate::ui::offscreen::WidgetTexture;
use crate::ui::outputs::Outputs;
use crate::ui::view::{PaintCtx, View};

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

fn temporal_mix_render(world: &World, entity: Entity, paint: &mut PaintCtx<'_>) {
    let Some(tm) = world.get::<TemporalMix>(entity) else {
        return;
    };
    let Some(current) = world
        .resource::<Outputs<WidgetTexture>>()
        .and_then(|outputs| outputs.get(tm.source))
    else {
        return;
    };
    let curr = current.texture();
    if curr.format != ColorFormat::RGBA8888 {
        return;
    }

    let mut tmp = Texture::owned(curr.width, curr.height, ColorFormat::RGBA8888);
    if let crate::render::texture::TexBuf::Owned(ref mut dst) = tmp.buf {
        dst.copy_from_slice(curr.buf.as_slice());
    }
    drop(current);

    // Read this widget's frozen output history for the current drawing batch.
    if let Some(previous) = world
        .resource::<Outputs<WidgetTexture>>()
        .and_then(|outputs| outputs.previous(entity))
    {
        let p = previous.texture();
        if p.width == tmp.width && p.height == tmp.height && p.format == tmp.format {
            mix_inplace(&mut tmp, p, tm.mix);
        }
    }

    let rect = paint.rect();
    paint.draw(&DrawCommand::Blit {
        pos: Point::new(rect.x, rect.y),
        size: Point::new(rect.w, rect.h),
        transform: paint.transform(),
        quad: paint.quad(),
        texture: &tmp,
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    });
}

pub fn view() -> View {
    View::new("TemporalMix", 60, temporal_mix_render).with_filter::<TemporalMix>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[crate::compose]
    fn temporal_fixture() {
        ui! {
            Row (width: 16, height: 8) {
                View (id: "temporal_source", width: 8, height: 8, bg_color: Color::rgb(255, 0, 0))
                View (id: "temporal_output", width: 8, height: 8) [
                    TemporalMix::new(id("temporal_source")).with_mix(128),
                ]
            }
        };
    }

    #[test]
    fn temporal_mix_retains_its_own_completed_output_between_batches() {
        let mut app = App::headless(16, 8);
        app.with_default_widgets()
            .with_offscreen_pool_budget(2 * 1024);
        let root = app.spawn_root().id();
        app.compose(root, temporal_fixture());
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
        app.compose(root, temporal_fixture());
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
}
