use crate::ecs::{Entity, World};
use crate::types::{Rect, Transform, Transform3D};
use crate::ui::view::ViewRegistry;
use alloc::vec::Vec;
use core::cell::Cell;

use super::PaintCoverage;

pub(super) struct PreparedPaint {
    coverage: Vec<Cell<Option<PaintCoverage>>>,
    views: Vec<Cell<usize>>,
    view_count: usize,
    affine: Vec<Cell<Option<Transform>>>,
}

impl PreparedPaint {
    pub(super) fn with_capacity(capacity: usize) -> Self {
        Self {
            coverage: Vec::with_capacity(capacity),
            views: Vec::new(),
            view_count: 0,
            affine: Vec::with_capacity(capacity),
        }
    }

    pub(super) fn resize(&mut self, len: usize, view_count: usize) {
        self.coverage.resize_with(len, || Cell::new(None));
        self.affine.resize_with(len, || Cell::new(None));
        self.view_count = view_count;
        self.views
            .resize_with(len * self.view_stride(), || Cell::new(0));
        self.clear();
    }

    pub(super) fn clear(&self) {
        self.clear_coverage();
        for entry in &self.affine {
            entry.set(None);
        }
        for entry in &self.views {
            entry.set(0);
        }
    }

    pub(super) fn clear_coverage(&self) {
        for entry in &self.coverage {
            entry.set(None);
        }
    }

    fn view_stride(&self) -> usize {
        1 + self.view_count.div_ceil(usize::BITS as usize)
    }

    pub(super) fn matching_views(
        &self,
        world: &World,
        registry: &ViewRegistry,
        entity: Entity,
        position: usize,
    ) -> Option<PreparedViews<'_>> {
        if self.view_count != registry.paint_iter().len() {
            return None;
        }
        let stride = self.view_stride();
        let row = self.views.get(position * stride..(position + 1) * stride)?;
        if row[0].get() == 0 {
            for (index, view) in registry.paint_iter().enumerate() {
                if view
                    .component_filter()
                    .is_none_or(|kind| world.has_type(entity, kind))
                {
                    let word = &row[1 + index / usize::BITS as usize];
                    word.set(word.get() | (1 << (index % usize::BITS as usize)));
                }
            }
            row[0].set(1);
        }
        Some(PreparedViews { words: &row[1..] })
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

pub(super) fn transforms(
    parent_affine: Transform,
    parent_projective: Transform3D,
    world: &World,
    entity: Entity,
    rect: Rect,
    position: usize,
    preparation: Option<&PreparedPaint>,
) -> (Transform, Transform3D) {
    let entry = preparation.and_then(|prepared| prepared.affine.get(position));
    if let Some(affine) = entry.and_then(Cell::get) {
        return (affine, Transform3D::IDENTITY);
    }
    let (affine, projective) =
        super::render_transforms(parent_affine, parent_projective, world, entity, rect);
    if projective.is_identity()
        && let Some(entry) = entry
    {
        entry.set(Some(affine));
    }
    (affine, projective)
}

pub(super) struct PreparedViews<'a> {
    words: &'a [Cell<usize>],
}

impl PreparedViews<'_> {
    pub(super) fn contains(&self, index: usize) -> bool {
        self.words[index / usize::BITS as usize].get() & (1 << (index % usize::BITS as usize)) != 0
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
    fn successful_frames_acknowledge_the_measured_coverage() {
        let mut app = App::headless(32, 32);
        app.with_default_widgets();
        app.with_widget(probe_render::view().with_measurement(measure));
        let root = app.spawn_root().id();
        app.compose(root, scene());
        app.render().unwrap();
        let entity = app.world.find_by_id("probe").unwrap();
        for full in [false, true] {
            app.world.get::<Probe>(entity).unwrap().measurements.set(0);
            app.world.invalidate_visual(entity);
            if full {
                app.render().unwrap();
            } else {
                app.render_dirty().unwrap();
            }
            assert_eq!(
                app.world.get::<Probe>(entity).unwrap().measurements.get(),
                2,
                "full={full}"
            );
        }
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
    fn affine_preparation_resets_between_batches_and_preserves_projective_composition() {
        use crate::ui::widgets::{WidgetTransform, WidgetTransform3D};
        let mut world = World::new();
        let entity = world.spawn_empty();
        let mut prepared = PreparedPaint::with_capacity(1);
        prepared.resize(1, 0);
        let rect = Rect::new(7, 11, 29, 31);
        let parent = Transform::rotate_deg(Fixed::from_int(19));
        for local in [
            Transform::translate(Fixed::HALF, Fixed::from_int(-3)),
            Transform::scale(Fixed::HALF, Fixed::from_int(2)),
        ] {
            world.insert(entity, WidgetTransform(local));
            prepared.clear();
            let expected = crate::ui::geometry::render_transforms(
                parent,
                Transform3D::IDENTITY,
                &world,
                entity,
                rect,
            );
            assert_eq!(
                crate::test_support::tracked_allocations(|| {
                    for _ in 0..16 {
                        assert_eq!(
                            transforms(
                                parent,
                                Transform3D::IDENTITY,
                                &world,
                                entity,
                                rect,
                                0,
                                Some(&prepared)
                            ),
                            expected
                        );
                    }
                }),
                0
            );
            assert_eq!(prepared.affine[0].get(), Some(expected.0));
        }
        prepared.clear();
        world.insert(
            entity,
            WidgetTransform3D(Transform3D::rotate_y_perspective(
                Fixed::from_int(35),
                Fixed::from_int(100),
            )),
        );
        let expected = crate::ui::geometry::render_transforms(
            parent,
            Transform3D::IDENTITY,
            &world,
            entity,
            rect,
        );
        assert_eq!(
            transforms(
                parent,
                Transform3D::IDENTITY,
                &world,
                entity,
                rect,
                0,
                Some(&prepared)
            ),
            expected
        );
        assert!(prepared.affine[0].get().is_none());
        assert_eq!(core::mem::size_of::<Cell<Option<Transform>>>(), 28);
    }

    #[test]
    fn view_membership_spans_words_and_refreshes_between_batches() {
        use crate::ui::view::View;
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(
            entity,
            Probe {
                measurements: Cell::new(0),
                global: Cell::new(false),
            },
        );
        let mut registry = ViewRegistry::default();
        for index in 0..usize::BITS as usize * 2 + 3 {
            let view = View::new("probe", (index % 7) as u8, |_, _, _| {});
            registry.insert(if index % 2 == 0 {
                view.with_filter::<Probe>()
            } else {
                view.with_filter::<crate::ui::Style>()
            });
        }
        let mut prepared = PreparedPaint::with_capacity(1);
        prepared.resize(1, registry.paint_iter().len());
        let check = |prepared: &PreparedPaint, world: &World, registry: &ViewRegistry| {
            let matching = prepared.matching_views(world, registry, entity, 0).unwrap();
            for (index, view) in registry.paint_iter().enumerate() {
                assert_eq!(
                    matching.contains(index),
                    world.has_type(entity, view.component_filter().unwrap())
                );
            }
        };
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                check(&prepared, &world, &registry);
                check(&prepared, &world, &registry);
            }),
            0
        );
        world.remove::<Probe>(entity);
        world.insert(entity, crate::ui::Style::default());
        prepared.clear();
        check(&prepared, &world, &registry);
        registry.insert(View::new("extra", 2, |_, _, _| {}).with_filter::<crate::ui::Style>());
        assert!(
            prepared
                .matching_views(&world, &registry, entity, 0)
                .is_none()
        );
        prepared.resize(1, registry.paint_iter().len());
        check(&prepared, &world, &registry);
    }

    #[test]
    fn batch_reset_and_retained_node_capacity_do_not_allocate() {
        let mut prepared = PreparedPaint::with_capacity(8);
        prepared.resize(8, 17);
        let coverage = PaintCoverage::solid(Rect::new(1, 2, 3, 4));
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..16 {
                    prepared.set(7, coverage);
                    assert_eq!(prepared.get(7), Some(coverage));
                    prepared.clear();
                    assert_eq!(prepared.get(7), None);
                    prepared.resize(4, 17);
                    prepared.resize(8, 17);
                    assert_eq!(prepared.get(7), None);
                }
            }),
            0
        );
        assert_eq!(core::mem::size_of::<Cell<Option<PaintCoverage>>>(), 36);
    }
}
