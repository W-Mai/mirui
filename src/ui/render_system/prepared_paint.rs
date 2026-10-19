use alloc::vec::Vec;
use core::cell::Cell;

use super::PaintCoverage;

pub(super) struct PreparedPaint {
    coverage: Vec<Cell<Option<PaintCoverage>>>,
}

impl PreparedPaint {
    pub(super) fn with_capacity(capacity: usize) -> Self {
        Self {
            coverage: Vec::with_capacity(capacity),
        }
    }

    pub(super) fn resize(&mut self, len: usize) {
        self.coverage.resize_with(len, || Cell::new(None));
        self.clear();
    }

    pub(super) fn clear(&self) {
        for entry in &self.coverage {
            entry.set(None);
        }
    }

    pub(super) fn get(&self, position: usize) -> Option<PaintCoverage> {
        self.coverage.get(position).and_then(Cell::get)
    }

    pub(super) fn set(&self, position: usize, coverage: PaintCoverage) {
        if let Some(entry) = self.coverage.get(position) {
            entry.set(Some(coverage));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;
    use crate::prelude::*;
    use crate::types::Rect;
    use crate::types::Viewport;
    use crate::ui::view::{PaintMeasurement, SourceGeometry};

    #[crate::component]
    struct Probe {
        measurements: Cell<usize>,
        global: Cell<bool>,
    }

    #[crate::view(component = Probe)]
    fn probe_render(probe: component!(), nodes: query!(Probe), entity: entity!()) {
        if probe.global.get() {
            for _ in nodes {}
        } else {
            let _ = nodes.get(entity);
        }
    }

    fn measure(
        world: &World,
        entity: Entity,
        rect: Rect,
        _: &dyn Fn(Entity) -> Option<SourceGeometry>,
    ) -> Option<PaintMeasurement> {
        let probe = world.get::<Probe>(entity)?;
        probe.measurements.set(probe.measurements.get() + 1);
        Some(PaintMeasurement {
            coverage: PaintCoverage::solid(rect),
            workspace: None,
        })
    }

    #[crate::compose]
    fn scene() {
        ui! {
            View (id: "probe", width: 20, height: 20) [
                Probe { measurements: Cell::new(0), global: Cell::new(false) },
            ]
        };
    }

    #[test]
    fn only_observed_local_measurements_are_reused_within_a_paint_batch() {
        let mut app = App::headless(32, 32);
        app.with_default_widgets();
        app.with_widget(probe_render::view().with_measurement(measure));
        let root = app.spawn_root().id();
        app.compose(root, scene());
        app.render().unwrap();
        let entity = app.world.find_by_id("probe").unwrap();
        let viewport = Viewport::new(32, 32, Fixed::ONE);
        let probe = app.world.get::<Probe>(entity).unwrap();
        let mut bytes = [0; 32 * 32 * 4];
        let texture = crate::render::texture::Texture::new(
            &mut bytes,
            32,
            32,
            crate::render::texture::ColorFormat::RGBA8888,
        )
        .unwrap();
        let mut renderer = crate::render::sw::SwRenderer::new(texture);
        for (global, reusable) in [(false, true), (true, false), (false, false)] {
            probe.global.set(global);
            super::super::render_region(
                &app.world,
                root,
                &viewport,
                &Rect::new(0, 0, 32, 32),
                &mut renderer,
            )
            .unwrap();
            probe.measurements.set(0);
            for expected in [1, 2] {
                let requests = super::super::prepare_paint_requests(&app.world).unwrap();
                for x in [0, 8, 16] {
                    super::super::render_region_cached(
                        &app.world,
                        app.world
                            .resource::<super::super::LayoutSnapshot>()
                            .unwrap(),
                        &Rect::new(x, 0, 8, 32),
                        &mut renderer,
                        &requests,
                        None,
                    )
                    .unwrap();
                }
                assert_eq!(
                    probe.measurements.get(),
                    expected * if reusable { 1 } else { 3 },
                    "global={global}, reusable={reusable}"
                );
            }
        }
    }

    #[test]
    fn successful_partial_frame_acknowledges_the_measured_coverage() {
        let mut app = App::headless(32, 32);
        app.with_default_widgets();
        app.with_widget(probe_render::view().with_measurement(measure));
        let root = app.spawn_root().id();
        app.compose(root, scene());
        app.render().unwrap();
        let entity = app.world.find_by_id("probe").unwrap();
        app.world.get::<Probe>(entity).unwrap().measurements.set(0);
        app.world.invalidate_visual(entity);
        app.render_dirty().unwrap();
        assert_eq!(
            app.world.get::<Probe>(entity).unwrap().measurements.get(),
            2
        );
        let snapshot = app
            .world
            .resource::<super::super::LayoutSnapshot>()
            .unwrap();
        let position = snapshot.entities.iter().position(|&e| e == entity).unwrap();
        assert!(snapshot.prepared_paint.get(position).is_none());

        snapshot.prepared_paint.set(position, PaintCoverage::EMPTY);
        super::super::seed_prev_rects(&mut app.world, root, &Viewport::new(32, 32, Fixed::ONE));
        assert_eq!(
            app.world.get::<Probe>(entity).unwrap().measurements.get(),
            3
        );
        assert!(
            !app.world
                .get::<crate::ui::dirty::PaintedCoverage>(entity)
                .unwrap()
                .0
                .is_empty()
        );
    }

    #[test]
    fn batch_reset_and_retained_node_capacity_do_not_allocate() {
        let mut prepared = PreparedPaint::with_capacity(8);
        prepared.resize(8);
        let coverage = PaintCoverage::solid(Rect::new(1, 2, 3, 4));
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..16 {
                    prepared.set(7, coverage);
                    assert_eq!(prepared.get(7), Some(coverage));
                    prepared.clear();
                    assert_eq!(prepared.get(7), None);
                    prepared.resize(4);
                    prepared.resize(8);
                    assert_eq!(prepared.get(7), None);
                }
            }),
            0
        );
        assert_eq!(core::mem::size_of::<Cell<Option<PaintCoverage>>>(), 36);
    }
}
