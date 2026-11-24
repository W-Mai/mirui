use super::{App, RenderViewport, clear_hidden_root, plugins};
use crate::ecs::{FrameOperation, FramePhase, TimingOutcome};
use crate::render::canvas::Canvas;
use crate::render::factory::RendererFactory;
use crate::render::renderer::Renderer;
use crate::surface::Surface;
use crate::types::{PhysicalRect, Rect};
use crate::ui::offscreen::WidgetTexture;
use crate::ui::outputs::Outputs;
use crate::ui::render_system;
use crate::ui::view::ViewRegistry;

impl<B: Surface, F: RendererFactory<B>> App<B, F> {
    pub(super) fn prepare_dirty_storage(&mut self) {
        let slots = self.world.allocated_entity_slots();
        let live_entities = self.world.live_entity_count();
        if slots <= self.dirty_reserved_slots && live_entities <= self.dirty_reserved_live_entities
        {
            return;
        }
        let Some(max_entity_id) = slots.checked_sub(1) else {
            return;
        };
        let current = self
            .world
            .storage::<crate::ui::dirty::Dirty>()
            .map_or(0, |storage| storage.len());
        self.world
            .reserve_component_storage::<crate::ui::dirty::Dirty>(
                max_entity_id,
                live_entities.saturating_sub(current),
            );
        let visual = self
            .world
            .storage::<crate::ui::dirty::VisualDirty>()
            .map_or(0, |storage| storage.len());
        self.world
            .reserve_component_storage::<crate::ui::dirty::VisualDirty>(
                max_entity_id,
                live_entities.saturating_sub(visual),
            );
        self.dirty_reserved_slots = slots;
        self.dirty_reserved_live_entities = live_entities;
        self.world.reserve_dirty_traversal_stack();
    }

    pub(super) fn sync_surface_geometry(&mut self, viewport: crate::types::Viewport) {
        let old_viewport = {
            let current = self
                .world
                .resource_mut::<RenderViewport>()
                .expect("App always owns a RenderViewport");
            let old = current.0;
            current.0 = viewport;
            old
        };
        let safe_area = self.backend.safe_area_insets();
        let old_safe_area = {
            let current = self
                .world
                .resource_mut::<crate::surface::SafeAreaInsets>()
                .expect("App always owns SafeAreaInsets");
            let old = *current;
            *current = safe_area;
            old
        };
        let viewport_changed = old_viewport != viewport;
        let safe_area_changed = old_safe_area != safe_area;
        if let Some(info) = self.world.resource_mut::<crate::surface::DisplayInfo>() {
            *info = self.backend.display_info();
        }
        if viewport_changed || safe_area_changed {
            // Geometry from the previous viewport is stale, but its layout
            // storage can be reused when the same root is recomputed.
            if let Some(snapshot) = self
                .world
                .resource_mut::<crate::ui::render_system::LayoutSnapshot>()
            {
                snapshot.invalidate();
            }
            if let Some(root) = self.root {
                self.world.mark_subtree_dirty(root);
            }
        }
        if viewport_changed {
            self.needs_full_redraw = true;
            self.notify_app(crate::input::event::lifecycle::ViewportChanged {
                old: old_viewport,
                new: viewport,
            });
        }
        if safe_area_changed {
            self.notify_app(crate::input::event::lifecycle::SafeAreaChanged {
                old: old_safe_area,
                new: safe_area,
            });
        }
    }

    pub(super) fn prepare_render(&mut self, viewport: crate::types::Viewport) {
        self.prepare_dirty_storage();
        self.sync_surface_geometry(viewport);
        for plugin in &mut self.plugins {
            plugin.pre_render(&mut self.world);
        }
        self.prepare_pending_paint();
    }

    fn prepare_pending_paint(&mut self) {
        self.flush_event_notifications();
        self.prepare_dirty_storage();
        ViewRegistry::reconcile_observations(&mut self.world);
        crate::ui::flush_bindings(&mut self.world);
        if let Some(wake) = self
            .world
            .resource::<alloc::rc::Rc<crate::core::reactive::PaintWake>>()
        {
            let parent = self
                .world
                .resource::<plugins::resources::ResourceHostWake>()
                .map_or(core::task::Waker::noop(), |host| &host.0);
            wake.register(parent);
        }
        crate::ui::offscreen::reconcile_texture_outputs(&self.world);
    }

    /// Render one frame
    pub fn render(&mut self) -> Result<(), crate::render::RenderError> {
        self.with_timing(FrameOperation::Draw, |app| {
            let result = app.render_frame(|_, _| Ok(())).map(|_| ());
            app.record_render_result(&result);
            result
        })
    }

    /// Render and present one frame, returning its pixels before the renderer releases the target. The texture retains the backend's pixel format and stride. No root returns `Ok(None)`; unsupported readback returns `RenderError::Unsupported` without substituting another renderer.
    ///
    /// Runs the same preparation and hooks as [`Self::render`], without advancing systems or polling input. A readback failure still completes presentation and records the successfully drawn frame. Unlike [`Self::snapshot_widget`], this reads the frame target instead of requesting a component's offscreen output.
    pub fn snapshot(
        &mut self,
    ) -> Result<Option<crate::render::texture::Texture<'static>>, crate::render::RenderError> {
        self.with_timing(FrameOperation::Draw, |app| {
            let result = app.render_frame(|renderer, viewport| {
                let (width, height) = viewport.physical_size();
                let area = viewport.logical_cover(PhysicalRect::from_size(width, height));
                renderer.prepare_readback(&area)?;
                let texture = renderer
                    .sample_target_region(&area)?
                    .ok_or(crate::render::RenderError::BackendFailure)?;
                if (texture.width, texture.height) != viewport.physical_size()
                    || !texture.valid_storage()
                {
                    return Err(crate::render::RenderError::BackendFailure);
                }
                Ok(texture)
            });
            app.record_render_result(&result);
            result
        })
    }

    #[mirui::trace_fn("frame.full")]
    pub(super) fn render_frame<R>(
        &mut self,
        finish: impl FnOnce(
            &mut dyn Renderer,
            &crate::types::Viewport,
        ) -> Result<R, crate::render::RenderError>,
    ) -> Result<Option<R>, crate::render::RenderError> {
        let _owner_scope = crate::core::reactive::OwnerGuard::enter(&mut self.world);
        let Some(root) = self.root else {
            return Ok(None);
        };
        self.prepare_text_layout()
            .map_err(|_| crate::render::RenderError::TextPreparation)?;
        let transform = self.backend.viewport();
        self.prepare_text_raster_scratch(&transform);
        self.prepare_render(transform);
        if self.text_content_failure(true).is_some() {
            self.world.mark_subtree_dirty(root);
            return Err(crate::render::RenderError::TextContent);
        }
        let root_hidden = crate::ui::branch::is_effectively_hidden(&self.world, root);

        let layout_start = self.timing_clock_ns();
        self.text_layout_started = true;

        let layout_result = {
            crate::trace_span!("frame.layout");
            render_system::try_collect_dirty_regions_into(
                &mut self.world,
                root,
                &transform,
                &mut self.dirty_plan,
            )
            .map_err(|_| crate::render::RenderError::TextLayout)
        };
        let layout_end = self.timing_clock_ns();
        self.record_phase(FramePhase::Layout, layout_start, layout_end);
        layout_result?;

        let requests = match render_system::prepare_paint_requests(&self.world) {
            Ok(requests) => requests,
            Err(error) => {
                self.record_phase(FramePhase::Render, layout_end, self.timing_clock_ns());
                return Err(error);
            }
        };
        if let Err(error) = self.backend.begin_flush() {
            self.record_phase(FramePhase::Render, layout_end, self.timing_clock_ns());
            self.backend.abort_flush();
            return Err(error);
        }
        self.needs_full_redraw = true;
        let render_result = {
            crate::trace_span!("frame.render");
            let _outputs_batch = self
                .world
                .resource::<Outputs<WidgetTexture>>()
                .map(Outputs::begin_batch);
            let mut renderer = self.factory.make(&mut self.backend, &transform);
            let result = if root_hidden {
                clear_hidden_root(&self.world, &transform, &mut renderer);
                Ok(())
            } else {
                let (width, height) = transform.physical_size();
                let area = PhysicalRect::from_size(width, height);
                renderer.clear(&area, &crate::types::Color::rgba(0, 0, 0, 0));
                let (width, height) = transform.logical_size();
                let rect = Rect::new(0, 0, width, height);
                if let Some(snapshot) = self.world.resource::<render_system::LayoutSnapshot>() {
                    render_system::render_region_cached(
                        &self.world,
                        snapshot,
                        &rect,
                        &mut renderer,
                        &requests,
                        Some(area),
                    )
                } else {
                    render_system::render_region_in_batch(
                        &self.world,
                        root,
                        &transform,
                        &rect,
                        &mut renderer,
                        &requests,
                        Some(area),
                    )
                }
            };
            result.and_then(|()| {
                let output = finish(&mut renderer, &transform);
                Renderer::flush(&mut renderer)?;
                Ok(output)
            })
        };
        let frame_output = match render_result {
            Ok(output) => output,
            Err(error) => {
                self.record_phase(FramePhase::Render, layout_end, self.timing_clock_ns());
                self.backend.abort_flush();
                self.world.mark_subtree_dirty(root);
                return Err(error);
            }
        };
        self.dirty_plan.clear();
        let (lw, lh) = transform.logical_size();
        self.dirty_plan.rects.push(Rect::new(0, 0, lw, lh));
        let render_end = self.timing_clock_ns();
        self.record_phase(FramePhase::Render, layout_end, render_end);
        let render_ns = layout_start
            .zip(render_end)
            .and_then(|(a, b)| b.checked_sub(a));

        let (pw, ph) = transform.physical_size();
        let submitted = (|| {
            crate::trace_span!("frame.flush");
            self.backend.flush(PhysicalRect::from_size(pw, ph))?;
            self.backend.end_flush()?;
            self.factory.mirror_buffers(
                &mut self.backend,
                &self.dirty_plan,
                &[PhysicalRect::from_size(pw, ph)],
                &transform,
            )
        })();
        if let Err(error) = submitted {
            self.record_phase(FramePhase::Flush, render_end, self.timing_clock_ns());
            self.backend.abort_flush();
            self.world.mark_subtree_dirty(root);
            return Err(error);
        }
        requests.commit();
        self.record_acceptance();
        render_system::acknowledge_rendered_outputs(&self.world);
        let flush_end = self.timing_clock_ns();
        self.record_phase(FramePhase::Flush, render_end, flush_end);

        // Seed PaintedCoverage so the next render_dirty frame's dirty union
        // covers pixels this full render actually wrote — otherwise
        // any widget that shrinks or moves between the full render
        // and the first dirty render leaves residue.
        render_system::acknowledge_painted_frame(&mut self.world, root, &transform);
        self.record_phase(FramePhase::SeedPrev, flush_end, self.timing_clock_ns());
        self.timing_outcome(TimingOutcome::Completed);
        self.dirty_plan.clear();
        self.needs_full_redraw = false;
        self.resource_wait_revision = None;
        self.painted_root_hidden = root_hidden;

        {
            crate::trace_span!("frame.finalize");
            self.publish_cache_stats();
        }
        {
            crate::trace_span!("frame.post_render");
            for p in &mut self.plugins {
                p.post_render(&mut self.world, render_ns);
            }
        }
        self.sync_platform_input();
        frame_output.map(Some)
    }

    pub(super) fn publish_cache_stats(&mut self) {
        let snapshots = crate::core::cache::InspectCaches::inspect_caches(&self.backend)
            .chain(crate::core::cache::InspectCaches::inspect_caches(
                &self.factory,
            ))
            .map(|(_, c)| crate::core::cache::CacheStatsSnapshot::capture(c));
        if let Some(registry) = self
            .world
            .resource_mut::<crate::core::cache::CacheRegistry>()
        {
            registry.refresh(snapshots);
        } else {
            let mut registry = crate::core::cache::CacheRegistry::default();
            registry.refresh(snapshots);
            self.world.insert_resource(registry);
        }
    }

    /// Get the bounding rectangle of pending damage in logical pixels without acknowledging drawing.
    ///
    /// Uses the same preparation and error handling as [`Self::dirty_regions`]. Returns `Ok(None)` when there is no root or no damage.
    pub fn dirty_region(&mut self) -> Result<Option<Rect>, crate::render::RenderError> {
        Ok(self.dirty_regions()?.bounding_rect())
    }

    /// Borrow pending redraw rectangles and scroll transfers in logical pixels.
    ///
    /// Prepares reactive state, surface geometry and layout without advancing systems, polling input or calling render hooks. Queries do not consume damage or advance painted geometry. Subsequent changes are included when drawing prepares its plan again; this is not a promise of the next surface flush rectangles.
    ///
    /// Returns an empty plan when there is no root or no damage. Preparation, content and layout failures use the same `RenderError` variants as rendering; layout details are available through [`Self::last_text_layout_failure`].
    pub fn dirty_regions(
        &mut self,
    ) -> Result<&crate::ui::dirty::DirtyRegions, crate::render::RenderError> {
        let _owner_scope = crate::core::reactive::OwnerGuard::enter(&mut self.world);
        let Some(root) = self.root else {
            self.dirty_plan.clear();
            return Ok(&self.dirty_plan);
        };
        self.prepare_text_layout()
            .map_err(|_| crate::render::RenderError::TextPreparation)?;
        let transform = self.backend.viewport();
        self.prepare_dirty_storage();
        self.sync_surface_geometry(transform);
        self.prepare_pending_paint();
        if self.text_content_failure(true).is_some() {
            self.world.mark_subtree_dirty(root);
            return Err(crate::render::RenderError::TextContent);
        }
        self.text_layout_started = true;
        render_system::try_collect_dirty_regions_into(
            &mut self.world,
            root,
            &transform,
            &mut self.dirty_plan,
        )
        .map_err(|_| crate::render::RenderError::TextLayout)?;
        if self.needs_full_redraw
            || (self.painted_root_hidden
                && !crate::ui::branch::is_effectively_hidden(&self.world, root))
            || self.backend.needs_redraw()
            || (!self.dirty_plan.is_empty()
                && self.backend.persistence() == crate::surface::BackbufferPersistence::Transient)
        {
            self.dirty_plan.clear();
            let (width, height) = transform.logical_size();
            self.dirty_plan.rects.push(Rect::new(0, 0, width, height));
        }
        Ok(&self.dirty_plan)
    }

    fn paint_revision(&self) -> (u64, u64, u64) {
        (
            self.world.component_revision::<crate::ui::dirty::Dirty>(),
            self.world
                .component_revision::<crate::ui::dirty::VisualDirty>(),
            self.world
                .resource::<crate::ui::dirty::ExactDirtyRegions>()
                .map_or(0, |regions| regions.revision()),
        )
    }

    /// Render only dirty regions. Falls back to full render if no dirty tracking.
    pub fn render_dirty(&mut self) -> Result<(), crate::render::RenderError> {
        self.with_timing(FrameOperation::Draw, |app| {
            let result = app.render_dirty_inner();
            app.record_render_result(&result);
            result
        })
    }

    #[mirui::trace_fn("frame.dirty")]
    fn render_dirty_inner(&mut self) -> Result<(), crate::render::RenderError> {
        let _owner_scope = crate::core::reactive::OwnerGuard::enter(&mut self.world);
        let Some(root) = self.root else { return Ok(()) };
        self.prepare_text_layout()
            .map_err(|_| crate::render::RenderError::TextPreparation)?;
        let transform = self.backend.viewport();
        self.prepare_text_raster_scratch(&transform);
        self.prepare_render(transform);
        let revision = self.paint_revision();
        if self.resource_wait_revision == Some(revision) {
            return Err(crate::render::RenderError::ResourcePending);
        }
        self.resource_wait_revision = None;
        if self.text_content_failure(true).is_some() {
            self.world.mark_subtree_dirty(root);
            return Err(crate::render::RenderError::TextContent);
        }
        let root_hidden = crate::ui::branch::is_effectively_hidden(&self.world, root);

        let layout_start = self.timing_clock_ns();
        self.text_layout_started = true;

        let mut plan = core::mem::take(&mut self.dirty_plan);
        let collected = crate::trace_span!("frame.collect_dirty", {
            render_system::try_collect_dirty_regions_into(
                &mut self.world,
                root,
                &transform,
                &mut plan,
            )
        });
        if collected.is_err() {
            self.record_phase(FramePhase::Layout, layout_start, self.timing_clock_ns());
            plan.clear();
            self.dirty_plan = plan;
            return Err(crate::render::RenderError::TextLayout);
        }
        let full_redraw = self.needs_full_redraw
            || (self.painted_root_hidden && !root_hidden)
            || self.backend.needs_redraw()
            || (!plan.is_empty()
                && (root_hidden
                    || self.backend.persistence()
                        == crate::surface::BackbufferPersistence::Transient));
        if full_redraw {
            plan.clear();
            let (lw, lh) = transform.logical_size();
            plan.rects.push(Rect::new(0, 0, lw, lh));
        }
        let layout_end = self.timing_clock_ns();
        self.record_phase(FramePhase::Layout, layout_start, layout_end);

        if !plan.is_empty() {
            let render_span = crate::core::perf::enter("frame.render_region");
            let requests = match render_system::prepare_paint_requests(&self.world) {
                Ok(requests) => requests,
                Err(error) => {
                    self.record_phase(FramePhase::Render, layout_end, self.timing_clock_ns());
                    self.dirty_plan = plan;
                    return Err(error);
                }
            };
            if let Err(error) = self.backend.begin_flush() {
                self.record_phase(FramePhase::Render, layout_end, self.timing_clock_ns());
                self.backend.abort_flush();
                self.dirty_plan = plan;
                return Err(error);
            }
            self.needs_full_redraw = true;
            let redraw_strategy = self.backend.redraw_strategy();
            let mut renderer = self.factory.make(&mut self.backend, &transform);

            let mut plan = plan;
            // Collection already normalized the logical damage. Only expanding
            // scroll transfers adds rectangles that need another union pass.
            if !renderer.supports_scroll_blit() && !plan.shifts.is_empty() {
                plan = plan.flatten_shifts();
                plan.coalesce_overlaps();
            }
            let physical = plan.physical_redraw(transform, full_redraw, redraw_strategy);

            if let Some(last) = self
                .world
                .resource_mut::<crate::ui::render_system::LastDirtyRegions>()
            {
                last.0.clone_from(&plan);
            } else {
                self.world
                    .insert_resource(crate::ui::render_system::LastDirtyRegions(plan.clone()));
            }
            let render_result = (|| {
                let _outputs_batch = self
                    .world
                    .resource::<Outputs<WidgetTexture>>()
                    .map(Outputs::begin_batch);
                for sop in &plan.shifts {
                    renderer.scroll_target_region(&sop.area, sop.dx, sop.dy)?;
                }

                let snapshot = self
                    .world
                    .resource::<crate::ui::render_system::LayoutSnapshot>();
                let result = if root_hidden {
                    clear_hidden_root(&self.world, &transform, &mut renderer);
                    Ok(())
                } else if let Some(snapshot) = snapshot {
                    render_system::render_regions_cached(
                        &self.world,
                        snapshot,
                        physical.as_slice(),
                        &transform,
                        &mut renderer,
                        &requests,
                    )
                } else {
                    let mut result = Ok(());
                    for &area in physical.as_slice() {
                        renderer.clear(&area, &crate::types::Color::rgba(0, 0, 0, 0));
                        let (width, height) = transform.logical_size();
                        let Some(rect) = transform
                            .logical_cover(area)
                            .intersect(&Rect::new(0, 0, width, height))
                        else {
                            continue;
                        };
                        result = render_system::render_region_in_batch(
                            &self.world,
                            root,
                            &transform,
                            &rect,
                            &mut renderer,
                            &requests,
                            Some(area),
                        );
                        if result.is_err() {
                            break;
                        }
                    }
                    result
                };
                result?;
                Renderer::flush(&mut renderer)?;
                Ok(())
            })();

            drop(renderer);
            drop(render_span);
            if let Err(error) = render_result {
                self.record_phase(FramePhase::Render, layout_end, self.timing_clock_ns());
                self.backend.abort_flush();
                self.world.mark_subtree_dirty(root);
                if error == crate::render::RenderError::ResourcePending {
                    self.resource_wait_revision = Some(self.paint_revision());
                }
                self.dirty_plan = plan;
                return Err(error);
            }

            let render_end = self.timing_clock_ns();
            self.record_phase(FramePhase::Render, layout_end, render_end);
            let render_ns = layout_start
                .zip(render_end)
                .and_then(|(a, b)| b.checked_sub(a));
            let submitted = (|| {
                crate::trace_span!("frame.flush");
                // A scrolled area is flushed whole, so damage inside it needs
                // no second transfer, and neither does a scrolled area that
                // damage around it has already sent.
                let in_scrolled = |area: crate::types::PhysicalRect| {
                    plan.shifts.iter().any(|sop| {
                        transform
                            .physical_rect(sop.area)
                            .is_some_and(|scrolled| scrolled.intersection(area) == Some(area))
                    })
                };
                for &area in physical.as_slice() {
                    if in_scrolled(area) {
                        continue;
                    }
                    self.backend.flush(area)?;
                }
                for sop in &plan.shifts {
                    if let Some(scrolled) = transform.physical_rect(sop.area) {
                        let sent = physical.as_slice().iter().any(|&area| {
                            !in_scrolled(area) && area.intersection(scrolled) == Some(scrolled)
                        });
                        if !sent {
                            self.backend.flush(scrolled)?;
                        }
                    }
                }
                self.backend.end_flush()?;
                self.factory.mirror_buffers(
                    &mut self.backend,
                    &plan,
                    physical.as_slice(),
                    &transform,
                )
            })();
            if let Err(error) = submitted {
                self.record_phase(FramePhase::Flush, render_end, self.timing_clock_ns());
                self.backend.abort_flush();
                self.world.mark_subtree_dirty(root);
                self.dirty_plan = plan;
                return Err(error);
            }
            requests.commit();
            self.record_acceptance();
            render_system::acknowledge_rendered_outputs(&self.world);
            self.record_phase(FramePhase::Flush, render_end, self.timing_clock_ns());
            let seed_start = self.timing_clock_ns();
            render_system::acknowledge_painted_frame(&mut self.world, root, &transform);
            self.record_phase(FramePhase::SeedPrev, seed_start, self.timing_clock_ns());
            self.timing_outcome(TimingOutcome::Completed);
            self.needs_full_redraw = false;
            self.resource_wait_revision = None;
            self.painted_root_hidden = root_hidden;

            {
                crate::trace_span!("frame.finalize");
                self.publish_cache_stats();
            }
            {
                crate::trace_span!("frame.post_render");
                for p in &mut self.plugins {
                    p.post_render(&mut self.world, render_ns);
                }
            }
            self.dirty_plan = plan;
            self.sync_platform_input();
            return Ok(());
        }

        // Idle frame: clear LastDirtyRegions so consumers (cursor
        // feedback, perf-plan-probe) can distinguish "this frame
        // produced no shift" from "stale plan from N frames ago".
        if let Some(last) = self
            .world
            .resource_mut::<crate::ui::render_system::LastDirtyRegions>()
        {
            last.0.clear();
        } else {
            self.world
                .insert_resource(crate::ui::render_system::LastDirtyRegions::default());
        }

        let render_end = self.timing_clock_ns();
        self.record_phase(FramePhase::Render, layout_end, render_end);
        let render_ns = layout_start
            .zip(render_end)
            .and_then(|(a, b)| b.checked_sub(a));
        self.timing_outcome(TimingOutcome::Idle);
        {
            crate::trace_span!("frame.finalize");
            self.publish_cache_stats();
        }
        {
            crate::trace_span!("frame.post_render");
            for p in &mut self.plugins {
                p.post_render(&mut self.world, render_ns);
            }
        }
        self.dirty_plan = plan;
        self.sync_platform_input();
        Ok(())
    }
}

pub(super) fn clone_texture_owned(
    src: &crate::render::texture::Texture<'static>,
) -> Result<crate::render::texture::Texture<'static>, crate::render::RenderError> {
    use crate::render::texture::Texture;
    let source = src.read_pixels()?;
    let mut owned = Texture::owned(source.width(), source.height(), src.format())?;
    owned.alpha_mode = src.alpha_mode;
    let mut target = owned.pixels_mut()?;
    if source.direct_format().is_none() {
        for y in 0..i32::from(source.height()) {
            for x in 0..i32::from(source.width()) {
                target.set_pixel(x, y, &source.get_pixel(x, y));
            }
        }
        return Ok(owned);
    }
    let stride = target.stride();
    for (row, destination) in target.bytes_mut().chunks_exact_mut(stride).enumerate() {
        let start = row * source.stride();
        destination.copy_from_slice(&source.bytes()[start..start + stride]);
    }
    Ok(owned)
}
