use super::*;
use crate::render::texture::TexturePixelsMut;
use crate::render::{
    BackendTarget, DrawCommand, DrawRequest, PlaneLayout, RenderRoute, Renderer, TargetAlpha,
    TargetDescriptor, TargetPlan, TargetUpdate,
};
use crate::test_support::tracked_allocations;
use crate::types::{Color, Fixed, Transform, Viewport};
use alloc::{boxed::Box, rc::Rc};
use core::{any::Any, cell::Cell};

mod consumers;

#[derive(Default)]
struct Device {
    lost: Cell<bool>,
    access_failure: Cell<bool>,
    read_failure: Cell<bool>,
    reads: Cell<usize>,
    creates: Cell<usize>,
    samples: Cell<usize>,
}

struct Contents {
    color: Cell<Color>,
    completed: Cell<bool>,
}

impl Default for Contents {
    fn default() -> Self {
        Self {
            color: Cell::new(Color::rgba(0, 0, 0, 0)),
            completed: Cell::new(false),
        }
    }
}

struct Target {
    device: Rc<Device>,
    contents: Rc<Contents>,
    descriptor: TargetDescriptor,
}

impl BackendTarget for Target {
    fn descriptor(&self) -> TargetDescriptor {
        self.descriptor
    }
    fn allocated_bytes(&self) -> usize {
        self.descriptor.layout.required_bytes()
    }
    fn validate(&self) -> Result<(), RenderError> {
        if self.device.access_failure.get() {
            Err(RenderError::InvalidScope)
        } else if self.device.lost.get() {
            Err(RenderError::BackendFailure)
        } else {
            Ok(())
        }
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn read_pixels_into(&self, pixels: &mut TexturePixelsMut<'_>) -> Result<(), RenderError> {
        self.validate()?;
        if !self.contents.completed.get() {
            return Err(RenderError::InvalidTexture);
        }
        self.device.reads.set(self.device.reads.get() + 1);
        for y in 0..i32::from(pixels.height()) {
            for x in 0..i32::from(pixels.width()) {
                pixels.set_pixel(x, y, &self.contents.color.get());
            }
        }
        if self.device.read_failure.get() {
            return Err(RenderError::BackendFailure);
        }
        Ok(())
    }
}

#[derive(Default)]
struct Fixture {
    device: Rc<Device>,
    active: Option<Rc<Contents>>,
    workspace_bytes: usize,
    bindings: usize,
    reject_storage: bool,
}

impl Renderer for Fixture {
    type TargetState = Option<Rc<Contents>>;
    fn plan_target(&self, layout: PlaneLayout) -> Result<Option<TargetPlan>, RenderError> {
        Ok(Some(TargetPlan {
            descriptor: TargetDescriptor {
                layout,
                alpha: TargetAlpha::Straight,
            },
            storage_bytes: layout.required_bytes(),
            workspace_bytes: self.workspace_bytes,
            readback_bytes: 64,
        }))
    }
    fn create_target(&mut self, plan: &TargetPlan) -> Result<Box<dyn BackendTarget>, RenderError> {
        self.device.creates.set(self.device.creates.get() + 1);
        Ok(Box::new(Target {
            device: Rc::clone(&self.device),
            contents: Rc::new(Contents::default()),
            descriptor: plan.descriptor,
        }))
    }
    fn can_reuse_target(
        &self,
        target: &dyn BackendTarget,
        plan: &TargetPlan,
    ) -> Result<bool, RenderError> {
        if self.device.lost.get() {
            return Err(RenderError::BackendFailure);
        }
        if self.device.access_failure.get() {
            return Err(RenderError::InvalidScope);
        }
        let Some(target) = target.as_any().downcast_ref::<Target>() else {
            return Ok(false);
        };
        if !Rc::ptr_eq(&self.device, &target.device)
            || self.reject_storage
            || target.descriptor != plan.descriptor
        {
            return Ok(false);
        }
        if self
            .active
            .as_ref()
            .is_some_and(|active| Rc::ptr_eq(active, &target.contents))
        {
            return Err(RenderError::InvalidScope);
        }
        Ok(self.plan_target(plan.descriptor.layout)? == Some(*plan))
    }
    fn bind_target(
        &mut self,
        target: &mut dyn BackendTarget,
        _: Viewport,
        update: TargetUpdate,
        workspace: &mut [u8],
    ) -> Result<Self::TargetState, RenderError> {
        let plan = self.plan_target(target.descriptor().layout)?.unwrap();
        if !self.can_reuse_target(target, &plan)? {
            return Err(RenderError::InvalidTexture);
        }
        let target = target
            .as_any()
            .downcast_ref::<Target>()
            .ok_or(RenderError::InvalidTexture)?;
        assert_eq!(update, TargetUpdate::Rebuild);
        assert_eq!(workspace.len(), self.workspace_bytes);
        workspace.fill(0x5a);
        self.bindings += 1;
        target.contents.completed.set(false);
        target.contents.color.set(Color::rgba(0, 0, 0, 0));
        Ok(self.active.replace(Rc::clone(&target.contents)))
    }
    fn restore_target(
        &mut self,
        previous: Self::TargetState,
        completed: bool,
    ) -> Result<(), RenderError> {
        self.active.take().unwrap().completed.set(completed);
        self.active = previous;
        Ok(())
    }
    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        request.validate()?;
        Ok(RenderRoute::Native)
    }
    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        let color = match request.command {
            DrawCommand::Fill { color, .. } => *color,
            DrawCommand::Blit { texture, .. } => {
                match texture
                    .backend_target()?
                    .and_then(|target| target.as_any().downcast_ref::<Target>())
                {
                    Some(target) if Rc::ptr_eq(&target.device, &self.device) => {
                        assert!(target.contents.completed.get());
                        self.device.samples.set(self.device.samples.get() + 1);
                        target.contents.color.get()
                    }
                    _ => texture.read_pixels()?.get_pixel(0, 0),
                }
            }
            _ if self.active.is_none() => return Ok(()),
            _ => return Err(RenderError::InvalidGeometry),
        };
        if let Some(active) = &self.active {
            active.color.set(color);
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<(), RenderError> {
        Ok(())
    }
    fn supports_offscreen(&self) -> bool {
        true
    }
    fn offscreen_format(&self) -> Option<ColorFormat> {
        Some(ColorFormat::RGBA8888)
    }
}

fn plan(renderer: &Fixture) -> TargetPlan {
    renderer
        .plan_target(PlaneLayout::packed(4, 4, ColorFormat::RGBA8888).unwrap())
        .unwrap()
        .unwrap()
}

#[test]
fn output_production_reserves_binding_and_paint_workspaces_together() {
    use crate::app::App;
    use crate::ecs::World;
    use crate::ui::dirty::PaintCoverage;
    use crate::ui::view::{PaintCtx, PaintMeasurement, SourceGeometry, View};
    struct Calls(Cell<usize>);

    fn measure(
        _: &World,
        _: Entity,
        rect: Rect,
        _: &dyn Fn(Entity) -> Option<SourceGeometry>,
    ) -> Option<PaintMeasurement> {
        Some(PaintMeasurement {
            coverage: PaintCoverage::solid(rect),
            workspace: Some(PlaneLayout::packed(4, 4, ColorFormat::RGBA8888).unwrap()),
        })
    }
    fn paint(world: &World, _: Entity, paint: &mut PaintCtx<'_>) {
        let calls = &world.resource::<Calls>().unwrap().0;
        calls.set(calls.get() + 1);
        let layout = PlaneLayout::packed(4, 4, ColorFormat::RGBA8888).unwrap();
        assert!(paint.with_workspace(layout, |texture, paint| {
            texture.pixels_mut().unwrap().bytes_mut().fill(0xa5);
            paint.draw(&DrawCommand::Fill {
                area: paint.rect(),
                transform: Transform::IDENTITY,
                quad: None,
                color: Color::rgb(20, 40, 60),
                radius: Fixed::ZERO,
                opa: 255,
            });
        }));
    }

    for budget in [287, 288] {
        let mut app = App::headless(4, 4);
        app.world.insert_resource(Calls(Cell::new(0)));
        app.with_default_widgets()
            .with_offscreen_pool_budget(budget);
        app.with_widget(View::new("Workspace", 90, paint).with_measurement(measure));
        let root = app.spawn_root().id();
        let config = crate::ui::OffscreenRender::new();
        app.world.insert(root, config);
        let mut renderer = Fixture {
            workspace_bytes: 32,
            ..Fixture::default()
        };
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        outputs.reserve(1).unwrap();
        outputs.set_retained(root, true).unwrap();
        crate::ui::render_system::render(
            &app.world,
            root,
            &Viewport::new(4, 4, Fixed::ONE),
            &mut renderer,
        )
        .unwrap();
        assert_eq!(app.world.resource::<Calls>().unwrap().0.get(), 1);
        if budget == 287 {
            assert_eq!(renderer.bindings, 0);
            assert_eq!(renderer.device.creates.get(), 0);
            let source = outputs.get(root).unwrap();
            assert!(!source.texture().is_native());
            assert_eq!(
                source.texture().read_pixels().unwrap().get_pixel(0, 0),
                Color::rgb(20, 40, 60)
            );
        } else {
            assert_eq!(renderer.bindings, 1);
            let source = outputs.get(root).unwrap();
            assert!(source.texture().is_native());
            assert_eq!(
                source.texture().read_pixels().unwrap().get_pixel(0, 0),
                Color::rgb(20, 40, 60)
            );
            drop(source);
            let allocated = outputs.storage().budget.allocated();
            crate::ui::render_system::render(
                &app.world,
                root,
                &Viewport::new(4, 4, Fixed::ONE),
                &mut renderer,
            )
            .unwrap();
            assert_eq!(renderer.bindings, 1);
            assert_eq!(outputs.storage().budget.allocated(), allocated);
        }
        outputs.trim_memory();
        assert_eq!(outputs.storage().budget.available(), budget);
    }
}

#[test]
fn automatic_admission_preserves_cpu_slots_and_shared_workspace_at_budget_boundaries() {
    for budget in [223, 224] {
        let outputs = Outputs::<WidgetTexture>::with_budget(budget);
        let mut working = outputs.take_texture_plan().unwrap();
        let key = super::tests::key(1, 1);
        let mut renderer = Fixture {
            workspace_bytes: 32,
            ..Fixture::default()
        };
        outputs.reserve(1).unwrap();
        assert!(
            outputs
                .reserve_render_texture(key, &mut renderer, &mut working.target_workspace, 0)
                .unwrap()
        );
        let native = outputs.storage().textures[&key.entity].planes[0]
            .as_ref()
            .unwrap()
            .texture
            .is_native();
        assert_eq!(native, budget == 224);
        let creates = renderer.device.creates.get();
        assert_eq!(
            tracked_allocations(|| {
                for _ in 0..8 {
                    assert!(
                        outputs
                            .reserve_render_texture(
                                key,
                                &mut renderer,
                                &mut working.target_workspace,
                                0
                            )
                            .unwrap()
                    );
                }
            }),
            0
        );
        assert_eq!(renderer.device.creates.get(), creates);
        if native {
            assert_eq!(working.target_workspace.capacity(), 32);
            renderer.device.access_failure.set(true);
            assert_eq!(
                outputs.reserve_render_texture(
                    key,
                    &mut renderer,
                    &mut working.target_workspace,
                    0
                ),
                Err(RenderError::InvalidScope)
            );
            assert_eq!(renderer.device.creates.get(), creates);
        }
        outputs.put_texture_plan(working);
    }
}

#[test]
fn automatic_cpu_selection_keeps_frozen_native_history() {
    let outputs = Outputs::<WidgetTexture>::with_budget(256);
    let mut working = outputs.take_texture_plan().unwrap();
    let key = super::tests::key(1, 1);
    outputs.reserve(1).unwrap();
    outputs.set_retained(key.entity, true).unwrap();
    let mut renderer = Fixture::default();
    let color = Color::rgb(15, 35, 75);
    {
        let _batch = outputs.begin_batch();
        assert!(
            outputs
                .reserve_render_texture(key, &mut renderer, &mut working.target_workspace, 0)
                .unwrap()
        );
        let mut write = outputs.prepare_texture(key).unwrap();
        assert!(write.texture_mut().is_native());
        draw(write.texture_mut(), &mut renderer, color, false).unwrap();
        assert!(write.publish());
    }
    let _batch = outputs.begin_batch();
    let changed = BufferKey {
        generation: 2,
        ..key
    };
    assert!(
        outputs
            .reserve_render_texture(changed, &mut renderer, &mut working.target_workspace, 0)
            .unwrap()
    );
    let mut write = outputs.prepare_texture(changed).unwrap();
    assert!(!write.texture_mut().is_native());
    write
        .texture_mut()
        .pixels_mut()
        .unwrap()
        .bytes_mut()
        .fill(0);
    assert!(write.publish());
    let previous = outputs.previous(key.entity).unwrap();
    assert!(previous.texture().is_native());
    assert_eq!(
        previous.texture().read_pixels().unwrap().get_pixel(0, 0),
        color
    );
    assert_eq!(renderer.device.creates.get(), 1);
    drop(previous);
    outputs.put_texture_plan(working);
}

#[test]
fn automatic_native_failure_runs_the_view_once_and_does_not_publish_or_retry_in_software() {
    use crate::app::App;
    use crate::ecs::World;
    use crate::ui::view::{PaintCtx, View};
    struct Calls(Cell<usize>);
    fn paint(world: &World, _: Entity, paint: &mut PaintCtx<'_>) {
        let calls = &world.resource::<Calls>().unwrap().0;
        calls.set(calls.get() + 1);
        paint.draw(&DrawCommand::Line {
            p1: crate::types::Point::ZERO,
            p2: crate::types::Point::new(2, 2),
            transform: Transform::IDENTITY,
            color: Color::rgb(20, 30, 40),
            width: Fixed::ONE,
            opa: 255,
        });
    }
    let mut app = App::headless(4, 4);
    app.with_default_widgets().with_offscreen_pool_budget(192);
    app.world.insert_resource(Calls(Cell::new(0)));
    app.with_widget(View::new("FailingNative", 90, paint));
    let root = app.spawn_root().id();
    app.world.insert(root, crate::ui::OffscreenRender::new());
    let mut renderer = Fixture::default();
    assert_eq!(
        crate::ui::render_system::render(
            &app.world,
            root,
            &Viewport::new(4, 4, Fixed::ONE),
            &mut renderer
        ),
        Err(RenderError::InvalidGeometry)
    );
    assert_eq!(app.world.resource::<Calls>().unwrap().0.get(), 1);
    assert_eq!(renderer.bindings, 1);
    assert!(renderer.active.is_none());
    assert!(
        app.world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(root)
            .is_none()
    );
}

fn draw(
    texture: &mut Texture<'_>,
    renderer: &mut dyn Renderer,
    color: Color,
    fail: bool,
) -> Result<(), RenderError> {
    texture.render_native(
        renderer,
        Viewport::new(4, 4, Fixed::ONE),
        &mut [],
        &mut |renderer| {
            renderer.submit(&DrawRequest::new(
                &DrawCommand::Fill {
                    area: Rect::new(0, 0, 4, 4),
                    transform: Transform::IDENTITY,
                    quad: None,
                    color,
                    radius: Fixed::ZERO,
                    opa: 255,
                },
                Rect::new(0, 0, 4, 4),
            ))?;
            if fail {
                Err(RenderError::InvalidGeometry)
            } else {
                Ok(())
            }
        },
    )
}

#[test]
fn source_pixels_are_lazy_borrowed_budgeted_and_survive_the_renderer() {
    let mut renderer = Fixture::default();
    let device = Rc::downgrade(&renderer.device);
    let budget = MemoryBudget::new(192);
    let plan = plan(&renderer);
    assert!(Texture::native(&mut renderer, plan, &MemoryBudget::new(191)).is_err());
    assert_eq!(renderer.device.creates.get(), 0);
    let mut texture = Texture::native(&mut renderer, plan, &budget).unwrap();
    assert_eq!(texture.cache_size(), 192);
    assert_eq!(budget.available(), 0);
    assert_eq!(budget.allocated(), 64);
    assert!(matches!(texture.pixels(), Err(RenderError::InvalidTexture)));
    assert!(matches!(
        texture.backend_target(),
        Err(RenderError::InvalidTexture)
    ));
    let color = Color::rgba(20, 40, 60, 128);
    draw(&mut texture, &mut renderer, color, false).unwrap();
    assert!(texture.pixels().unwrap().is_none());
    assert_eq!(renderer.device.reads.get(), 0);
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..8 {
                let source = texture.backend_target().unwrap().unwrap();
                assert!(source.as_any().is::<Target>());
                assert_eq!(source.descriptor(), plan.descriptor);
            }
        }),
        0
    );
    assert_eq!(renderer.device.reads.get(), 0);
    assert_eq!(budget.allocated(), 64);
    assert!(texture.pixels_mut().is_err());
    {
        let first = texture.read_pixels().unwrap();
        let second = texture.pixels().unwrap().unwrap();
        assert_eq!(first.bytes().as_ptr(), second.bytes().as_ptr());
        assert_eq!(first.get_pixel(3, 3), color);
        assert_eq!(budget.allocated(), 128);
        assert_eq!(budget.available(), 0);
        assert_eq!(texture.cache_size(), 192);
    }
    let allocations = tracked_allocations(|| {
        for _ in 0..8 {
            assert_eq!(texture.read_pixels().unwrap().get_pixel(1, 1), color);
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(renderer.device.reads.get(), 1);
    let address = texture.read_pixels().unwrap().bytes().as_ptr();
    let updated = Color::rgb(100, 120, 140);
    let allocations = tracked_allocations(|| {
        draw(&mut texture, &mut renderer, updated, false).unwrap();
        assert!(texture.pixels().unwrap().is_none());
        let pixels = texture.read_pixels().unwrap();
        assert_eq!(pixels.bytes().as_ptr(), address);
        assert_eq!(pixels.get_pixel(2, 2), updated);
    });
    assert_eq!(allocations, 0);
    drop(renderer);
    assert!(texture.backend_target().unwrap().is_some());
    assert_eq!(device.upgrade().unwrap().reads.get(), 2);
    assert_eq!(texture.read_pixels().unwrap().get_pixel(0, 0), updated);
    drop(texture);
    assert!(device.upgrade().is_none());
    assert_eq!(budget.available(), 192);
    assert_eq!(budget.allocated(), 0);
}

#[test]
fn failed_reads_and_writes_never_expose_partial_or_previous_pixels() {
    let mut renderer = Fixture::default();
    let plan = plan(&renderer);
    let mut texture = Texture::native(&mut renderer, plan, &MemoryBudget::new(192)).unwrap();
    let color = Color::rgb(1, 2, 3);
    draw(&mut texture, &mut renderer, color, false).unwrap();
    renderer.device.read_failure.set(true);
    assert!(matches!(
        texture.read_pixels(),
        Err(RenderError::BackendFailure)
    ));
    assert!(texture.pixels().unwrap().is_none());
    renderer.device.read_failure.set(false);
    assert_eq!(texture.read_pixels().unwrap().get_pixel(0, 0), color);
    assert_eq!(
        draw(&mut texture, &mut renderer, Color::rgb(40, 50, 60), true),
        Err(RenderError::InvalidGeometry)
    );
    assert!(matches!(
        texture.read_pixels(),
        Err(RenderError::InvalidTexture)
    ));
    assert!(matches!(
        texture.backend_target(),
        Err(RenderError::InvalidTexture)
    ));
    draw(&mut texture, &mut renderer, color, false).unwrap();
    texture.read_pixels().unwrap();
    renderer.device.lost.set(true);
    assert!(texture.backend_target().unwrap().is_none());
    assert_eq!(texture.read_pixels().unwrap().get_pixel(0, 0), color);
    renderer.device.access_failure.set(true);
    assert!(matches!(
        texture.backend_target(),
        Err(RenderError::InvalidScope)
    ));
    renderer.device.access_failure.set(false);
    texture.cache_revision += 1;
    assert!(matches!(
        texture.backend_target(),
        Err(RenderError::BackendFailure)
    ));
    assert!(matches!(
        texture.read_pixels(),
        Err(RenderError::BackendFailure)
    ));
}

#[test]
fn incompatible_fresh_target_releases_all_reservations() {
    let mut renderer = Fixture {
        reject_storage: true,
        ..Fixture::default()
    };
    let requested = plan(&renderer);
    let budget = MemoryBudget::new(192);
    assert!(matches!(
        Texture::native(&mut renderer, requested, &budget),
        Err(RenderError::InvalidTexture)
    ));
    assert_eq!(budget.available(), 192);
    assert_eq!(renderer.device.creates.get(), 1);
    assert_eq!(renderer.bindings, 0);
}

#[test]
fn cached_sources_remain_readable_across_consumers_but_invalid_sources_are_not_ready() {
    let outputs = Outputs::<WidgetTexture>::with_budget(192);
    let key = super::tests::key(1, 1);
    let mut producer = Fixture::default();
    let mut consumer = Fixture::default();
    let requested = plan(&producer);
    outputs
        .reserve_texture(key, Some((&mut producer, requested)))
        .unwrap();
    let mut write = outputs.prepare_texture(key).unwrap();
    draw(
        write.texture_mut(),
        &mut producer,
        Color::rgb(1, 2, 3),
        false,
    )
    .unwrap();
    assert!(write.publish());
    assert!(
        outputs
            .reserve_texture(key, Some((&mut consumer, requested)))
            .unwrap()
    );
    assert_eq!(consumer.device.creates.get(), 0);
    assert_eq!(producer.device.reads.get(), 0);
    producer.device.lost.set(true);
    assert_eq!(
        outputs.texture_matches(key),
        Err(RenderError::BackendFailure)
    );
    producer.device.lost.set(false);
    outputs
        .get(key.entity)
        .unwrap()
        .texture()
        .read_pixels()
        .unwrap();
    producer.device.lost.set(true);
    assert!(outputs.texture_matches(key).unwrap());
    assert!(
        outputs
            .reserve_texture(key, Some((&mut consumer, requested)))
            .unwrap()
    );
    assert_eq!(consumer.device.creates.get(), 0);
}

#[test]
fn native_write_slot_replacement_preserves_frozen_history_and_reuses_warm_storage() {
    let outputs = Outputs::<WidgetTexture>::with_budget(384);
    let key = super::tests::key(1, 1);
    outputs.reserve(1).unwrap();
    outputs.set_retained(key.entity, true).unwrap();
    let mut first = Fixture::default();
    let mut second = Fixture::default();
    for generation in 1..=5 {
        let _batch = outputs.begin_batch();
        let key = BufferKey { generation, ..key };
        let renderer = if generation < 3 {
            &mut first
        } else {
            &mut second
        };
        let requested = plan(renderer);
        if generation == 5 {
            assert_eq!(
                tracked_allocations(|| {
                    assert!(
                        outputs
                            .reserve_texture(key, Some((renderer, requested)))
                            .unwrap()
                    );
                }),
                0
            );
        } else {
            assert!(
                outputs
                    .reserve_texture(key, Some((renderer, requested)))
                    .unwrap()
            );
        }
        if generation > 1 {
            assert_eq!(
                outputs
                    .previous(key.entity)
                    .unwrap()
                    .texture()
                    .read_pixels()
                    .unwrap()
                    .get_pixel(0, 0),
                Color::rgb((generation - 1) as u8, 0, 0)
            );
        }
        let mut write = outputs.prepare_texture(key).unwrap();
        draw(
            write.texture_mut(),
            renderer,
            Color::rgb(generation as u8, 0, 0),
            false,
        )
        .unwrap();
        assert!(write.publish());
    }
    assert_eq!(first.device.creates.get(), 2);
    assert_eq!(second.device.creates.get(), 2);
    assert_eq!(
        outputs
            .get(key.entity)
            .unwrap()
            .texture()
            .read_pixels()
            .unwrap()
            .get_pixel(0, 0),
        Color::rgb(5, 0, 0)
    );
}

#[test]
fn failed_reuse_check_does_not_replace_storage_and_budget_failure_preserves_history() {
    let key = super::tests::key(1, 1);
    let outputs = Outputs::<WidgetTexture>::with_budget(383);
    let mut first = Fixture::default();
    let requested = plan(&first);
    outputs.reserve(1).unwrap();
    outputs.set_retained(key.entity, true).unwrap();
    {
        let _batch = outputs.begin_batch();
        outputs
            .reserve_texture(key, Some((&mut first, requested)))
            .unwrap();
        first.device.access_failure.set(true);
        assert_eq!(
            outputs.reserve_texture(key, Some((&mut first, requested))),
            Err(RenderError::InvalidScope)
        );
        assert_eq!(first.device.creates.get(), 1);
        first.device.access_failure.set(false);
        outputs
            .reserve_texture(key, Some((&mut first, requested)))
            .unwrap();
        assert_eq!(first.device.creates.get(), 1);
        let mut write = outputs.prepare_texture(key).unwrap();
        draw(
            write.texture_mut(),
            &mut first,
            Color::rgb(20, 30, 40),
            false,
        )
        .unwrap();
        assert!(write.publish());
    }
    let _batch = outputs.begin_batch();
    let mut second = Fixture::default();
    let changed = BufferKey {
        generation: 2,
        ..key
    };
    assert!(matches!(
        outputs.reserve_texture(changed, Some((&mut second, requested))),
        Err(RenderError::ResourceLimit(_))
    ));
    assert_eq!(second.device.creates.get(), 0);
    assert_eq!(
        outputs
            .previous(key.entity)
            .unwrap()
            .texture()
            .read_pixels()
            .unwrap()
            .get_pixel(0, 0),
        Color::rgb(20, 30, 40)
    );
}

#[test]
fn native_output_writes_share_publication_history_and_failed_slot_reuse() {
    let mut renderer = Fixture::default();
    let plan = plan(&renderer);
    let outputs = Outputs::<WidgetTexture>::with_budget(384);
    let source = super::tests::key(1, 1);
    outputs.reserve(1).unwrap();
    outputs.set_retained(source.entity, true).unwrap();
    let color = Color::rgb(10, 20, 30);
    {
        let _batch = outputs.begin_batch();
        assert!(
            outputs
                .reserve_texture(source, Some((&mut renderer, plan)))
                .unwrap()
        );
        let mut write = outputs.prepare_texture(source).unwrap();
        assert!(outputs.get(source.entity).is_none());
        draw(write.texture_mut(), &mut renderer, color, false).unwrap();
        write.texture_mut().read_pixels().unwrap();
        assert!(write.publish());
        let read = outputs.get(source.entity).unwrap();
        assert_eq!(read.texture().read_pixels().unwrap().get_pixel(0, 0), color);
        assert_eq!(renderer.device.reads.get(), 1);
    }
    {
        let _batch = outputs.begin_batch();
        let changed = BufferKey {
            generation: 2,
            ..source
        };
        assert!(
            outputs
                .reserve_texture(changed, Some((&mut renderer, plan)))
                .unwrap()
        );
        let mut write = outputs.prepare_texture(changed).unwrap();
        assert!(
            draw(
                write.texture_mut(),
                &mut renderer,
                Color::rgb(70, 80, 90),
                true
            )
            .is_err()
        );
        assert!(!write.publish());
        assert!(outputs.get(source.entity).is_none());
        let previous = outputs.previous(source.entity).unwrap();
        assert_eq!(
            previous.texture().read_pixels().unwrap().get_pixel(0, 0),
            color
        );
        drop(previous);
        let mut write = outputs.prepare_texture(changed).unwrap();
        draw(
            write.texture_mut(),
            &mut renderer,
            Color::rgb(70, 80, 90),
            false,
        )
        .unwrap();
        assert!(write.publish());
    }
    assert_eq!(renderer.device.creates.get(), 2);
    {
        let read = outputs.get(source.entity).unwrap();
        let pixels = read.texture().read_pixels().unwrap();
        let allocated = outputs.storage().budget.allocated();
        assert_eq!(tracked_allocations(|| outputs.trim_memory()), 0);
        assert_eq!(outputs.storage().budget.allocated(), allocated);
        assert_eq!(pixels.get_pixel(0, 0), Color::rgb(70, 80, 90));
    }
    outputs.trim_memory();
    assert_eq!(outputs.storage().budget.available(), 384);
}

#[test]
fn native_sources_use_the_existing_software_blit_and_error_path() {
    use crate::render::backends::sw::SwRenderer;
    use crate::render::command::CompositeMode;
    use crate::types::Point;

    let mut renderer = Fixture::default();
    let plan = plan(&renderer);
    let mut texture = Texture::native(&mut renderer, plan, &MemoryBudget::new(192)).unwrap();
    let color = Color::rgba(10, 20, 30, 128);
    draw(&mut texture, &mut renderer, color, false).unwrap();
    let mut target = Texture::owned(4, 4, ColorFormat::RGBA8888).unwrap();
    target.alpha_mode = crate::render::texture::AlphaMode::Blend;
    let mut software = SwRenderer::new(target);
    let blit = |software: &mut SwRenderer<'_>, texture: &Texture<'_>| {
        software.submit(&DrawRequest::new(
            &DrawCommand::Blit {
                pos: Point::new(0, 0),
                size: Point::new(4, 4),
                transform: Transform::IDENTITY,
                quad: None,
                texture,
                opa: 255,
                radius: Fixed::ZERO,
                composite: CompositeMode::SourceOver,
            },
            Rect::new(0, 0, 4, 4),
        ))
    };
    renderer.device.read_failure.set(true);
    assert_eq!(
        blit(&mut software, &texture),
        Err(RenderError::BackendFailure)
    );
    assert!(
        software
            .target
            .read_pixels()
            .unwrap()
            .bytes()
            .iter()
            .all(|byte| *byte == 0)
    );
    assert_eq!(
        Renderer::flush(&mut software),
        Err(RenderError::BackendFailure)
    );
    let mut software = SwRenderer::new(software.target);
    renderer.device.read_failure.set(false);
    blit(&mut software, &texture).unwrap();
    Renderer::flush(&mut software).unwrap();
    assert_eq!(
        software.target.read_pixels().unwrap().get_pixel(2, 2),
        color
    );
    assert_eq!(renderer.device.reads.get(), 2);
}

#[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
#[test]
fn native_upload_keys_retain_identity_without_retaining_the_device() {
    use crate::render::texture::TextureKey;

    let mut renderer = Fixture::default();
    let device = Rc::downgrade(&renderer.device);
    let plan = plan(&renderer);
    let budget = MemoryBudget::new(384);
    let mut first = Texture::native(&mut renderer, plan, &budget).unwrap();
    let second = Texture::native(&mut renderer, plan, &budget).unwrap();
    let key = TextureKey::from(&first);
    assert_ne!(key, TextureKey::from(&second));
    draw(&mut first, &mut renderer, Color::rgb(2, 3, 4), false).unwrap();
    first.read_pixels().unwrap();
    assert_eq!(key, TextureKey::from(&first));
    first.set_output_revision(7);
    assert_ne!(key, TextureKey::from(&first));
    assert!(first.pixels().unwrap().is_some());
    drop(first);
    let replacement = Texture::native(&mut renderer, plan, &budget).unwrap();
    assert_ne!(key, TextureKey::from(&replacement));
    drop((renderer, second, replacement));
    assert!(device.upgrade().is_none());
    assert_eq!(budget.available(), 384);
    drop(key);
}

#[test]
fn unwinding_native_production_invalidates_pixels_and_restores_the_parent() {
    let mut renderer = Fixture::default();
    let plan = plan(&renderer);
    let budget = MemoryBudget::new(192);
    let mut texture = Texture::native(&mut renderer, plan, &budget).unwrap();
    draw(&mut texture, &mut renderer, Color::rgb(1, 2, 3), false).unwrap();
    texture.read_pixels().unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = texture.render_native(
            &mut renderer,
            Viewport::new(4, 4, Fixed::ONE),
            &mut [],
            &mut |_| panic!("draw interrupted"),
        );
    }));
    assert!(result.is_err());
    assert!(renderer.active.is_none());
    assert!(matches!(
        texture.read_pixels(),
        Err(RenderError::InvalidTexture)
    ));
    draw(&mut texture, &mut renderer, Color::rgb(4, 5, 6), false).unwrap();
    assert_eq!(
        texture.read_pixels().unwrap().get_pixel(1, 1),
        Color::rgb(4, 5, 6)
    );
}
