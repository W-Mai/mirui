use super::*;
use crate::render::TargetPlan;
use crate::ui::offscreen::TextureBaseline;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Selection {
    Undecided,
    Cpu,
    Native,
}

pub(super) struct NativeAdmission {
    capture: usize,
    plan: Option<TargetPlan>,
    minimum: usize,
    baseline: TextureBaseline,
    selection: Selection,
}

impl TexturePlan {
    pub(super) fn admit_native_suffix(
        &mut self,
        start: usize,
        first: TargetPlan,
        outputs: &Outputs<WidgetTexture>,
        renderer: &mut dyn Renderer,
        binding_required: bool,
    ) -> Result<(), RenderError> {
        self.admissions.clear();
        let result = self.admit_native_inner(start, first, outputs, renderer, binding_required);
        self.admissions.clear();
        result
    }

    fn admit_native_inner(
        &mut self,
        start: usize,
        first: TargetPlan,
        outputs: &Outputs<WidgetTexture>,
        renderer: &mut dyn Renderer,
        binding_required: bool,
    ) -> Result<(), RenderError> {
        if outputs.reserve_texture_budget(0).is_none() {
            return self.admit_producer_suffix(start, first, outputs, renderer, binding_required);
        }
        let mut pending = 0;
        for position in start..self.graph.order().len() {
            let id = self.graph.order()[position];
            let state = self.prepare_capture(id, outputs)?;
            self.captures[id].state = state;
            pending += usize::from(state == CaptureState::Pending);
        }
        if self.admissions.try_reserve_exact(pending).is_err() {
            return self.admit_producer_suffix(start, first, outputs, renderer, binding_required);
        }
        for position in start..self.graph.order().len() {
            let id = self.graph.order()[position];
            if self.captures[id].state != CaptureState::Pending {
                continue;
            }
            if self
                .graph
                .dependencies(id)
                .any(|dependency| self.captures[dependency].state == CaptureState::Missing)
            {
                self.captures[id].state = CaptureState::Missing;
                outputs.invalidate(self.captures[id].entity);
                continue;
            }
            let key = self.captures[id].key;
            let target = if position == start {
                Some(first)
            } else {
                outputs.plan_render_texture(key, renderer)?
            };
            let Some(baseline) = outputs.texture_baseline(key, renderer, target.as_ref())? else {
                self.captures[id].state = CaptureState::Missing;
                outputs.invalidate(key.entity);
                continue;
            };
            let minimum = if baseline == TextureBaseline::AllocateCpu {
                PlaneLayout::packed(key.w, key.h, key.format)
                    .map_err(|_| RenderError::InvalidTexture)?
                    .required_bytes()
            } else {
                0
            };
            self.admissions.push(NativeAdmission {
                capture: id,
                plan: target,
                minimum,
                baseline,
                selection: Selection::Undecided,
            });
        }
        self.admissions.sort_unstable_by_key(|entry| entry.capture);
        let paint = self
            .workspace_bytes
            .saturating_sub(self.workspace.capacity());
        let minimum = self
            .admissions
            .iter()
            .try_fold(paint, |total, entry| total.checked_add(entry.minimum));
        let Some(mut floor) = minimum.and_then(|minimum| outputs.reserve_texture_budget(minimum))
        else {
            return self.admit_producer_suffix(start, first, outputs, renderer, binding_required);
        };
        // Materialize shared baseline binding storage once while pixel/paint minima are protected.
        let binding_bytes = self
            .admissions
            .iter()
            .filter(|entry| entry.baseline == TextureBaseline::ReuseNative)
            .filter_map(|entry| entry.plan.map(|plan| plan.workspace_bytes))
            .max()
            .unwrap_or(0);
        if binding_bytes > self.target_workspace.capacity()
            && !outputs.reserve_workspace(&mut self.target_workspace, binding_bytes)
        {
            drop(floor);
            return self.admit_producer_suffix(start, first, outputs, renderer, binding_required);
        }
        let mut binding_required = binding_required || binding_bytes != 0;
        for position in (start..self.graph.order().len()).rev() {
            let id = self.graph.order()[position];
            let Ok(index) = self
                .admissions
                .binary_search_by_key(&id, |entry| entry.capture)
            else {
                continue;
            };
            let entry = &self.admissions[index];
            let remaining = floor.bytes() - entry.minimum;
            assert!(
                floor.resize(remaining),
                "remaining output reservation must shrink"
            );
            let key = self.captures[id].key;
            let cpu_consumer = self.graph.consumers(id).any(|consumer| {
                self.admissions
                    .binary_search_by_key(&consumer, |entry| entry.capture)
                    .is_ok_and(|index| self.admissions[index].selection == Selection::Cpu)
            });
            let prefer_cpu = cpu_consumer
                && (entry.baseline != TextureBaseline::ReuseNative
                    || PlaneLayout::packed(key.w, key.h, key.format).is_ok_and(|layout| {
                        layout.required_bytes() <= outputs.texture_budget_available()
                    }));
            let target = if prefer_cpu { None } else { entry.plan };
            let native = target.map(|plan| (&mut *renderer as &mut dyn Renderer, plan));
            let admitted = outputs.reserve_render_texture(
                key,
                native,
                &mut self.target_workspace,
                0,
                binding_required,
                #[cfg(feature = "perf")]
                crate::ui::offscreen::AdmissionTrace {
                    protected: floor.bytes(),
                    cpu_consumer: prefer_cpu && entry.plan.is_some(),
                },
            )?;
            if admitted {
                binding_required |= outputs.write_workspace_bytes(key.entity) != 0;
                self.admissions[index].selection = if outputs.write_texture_is_native(key.entity) {
                    Selection::Native
                } else {
                    Selection::Cpu
                };
            } else {
                self.captures[id].state = CaptureState::Missing;
            }
        }
        // The caller allocates shared paint workspace before any View executes.
        drop(floor);
        Ok(())
    }

    fn admit_producer_suffix(
        &mut self,
        start: usize,
        first: TargetPlan,
        outputs: &Outputs<WidgetTexture>,
        renderer: &mut dyn Renderer,
        mut binding_required: bool,
    ) -> Result<(), RenderError> {
        self.admissions.sort_unstable_by_key(|entry| entry.capture);
        for position in start..self.graph.order().len() {
            let id = self.graph.order()[position];
            let mut state = self.prepare_capture(id, outputs)?;
            let key = self.captures[id].key;
            if state == CaptureState::Pending {
                let target = match self
                    .admissions
                    .binary_search_by_key(&id, |entry| entry.capture)
                {
                    Ok(index) => self.admissions[index].plan,
                    Err(_) if position == start => Some(first),
                    Err(_) => outputs.plan_render_texture(key, renderer)?,
                };
                let native = target.map(|plan| (&mut *renderer as &mut dyn Renderer, plan));
                if !outputs.reserve_render_texture(
                    key,
                    native,
                    &mut self.target_workspace,
                    self.workspace_bytes
                        .saturating_sub(self.workspace.capacity()),
                    binding_required,
                    #[cfg(feature = "perf")]
                    crate::ui::offscreen::AdmissionTrace::default(),
                )? {
                    state = CaptureState::Missing;
                }
            }
            if state == CaptureState::Pending {
                binding_required |= outputs.write_workspace_bytes(key.entity) != 0;
            }
            self.captures[id].state = state;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_planning_storage_has_bounded_layout_and_exits_with_the_tree() {
        assert_eq!(
            core::mem::size_of::<NativeAdmission>(),
            if core::mem::size_of::<usize>() == 8 {
                96
            } else {
                52
            }
        );
        let mut plan = TexturePlan::default();
        for trim in [false, true] {
            plan.admissions.try_reserve_exact(4).unwrap();
            for capture in 0..4 {
                plan.admissions.push(NativeAdmission {
                    capture,
                    plan: None,
                    minimum: 4,
                    baseline: TextureBaseline::AllocateCpu,
                    selection: Selection::Undecided,
                });
            }
            if trim {
                plan.trim_memory();
            } else {
                plan.invalidate_structure();
            }
            assert_eq!(plan.admissions.capacity(), 0);
        }
        plan.admissions.try_reserve_exact(4).unwrap();
        let mut world = World::new();
        let root = world.spawn_empty();
        let tree = LayoutNode::new(LayoutStyle::default());
        plan.build(&world, &tree, &[root]);
        assert_eq!(plan.admissions.capacity(), 0);
    }
}
