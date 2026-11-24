use crate::ecs::{Entity, World};
use crate::render::renderer::{RenderError, Renderer};
use crate::types::{Fixed, Point, Rect, Transform, Transform3D};
use crate::ui::layout::LayoutNode;
use alloc::vec::Vec;

use super::dirty::PaintCoverage;
use super::geometry::{render_transforms, transformed_clip_bounds};
use super::view::ViewRegistry;
use super::{Outputs, Style, WidgetTexture};

#[cfg(test)]
use crate::{
    core::reactive::PaintRequestScope,
    render::{
        BackendTarget, PlaneLayout, TargetPlan,
        command::DrawCommand,
        renderer::{DrawRequest, RenderRoute},
        texture::Texture,
    },
    text::TextGeometry,
    types::{Dimension, Viewport},
    ui::{
        Children, Parent, Widget,
        dirty::DirtyRegions,
        layout::LayoutStyle,
        state::{InteractionState, UserState},
        theme::WidgetState,
        view::PaintCtx,
    },
};
#[cfg(test)]
use alloc::boxed::Box;

mod damage;
mod layout_snapshot;
pub(crate) mod output_plan;
mod paint;
mod prepared_paint;
mod scroll;
mod text_layout;

pub(crate) use damage::{
    DIRTY_PLAN_RECT_CAPACITY, DIRTY_REGION_CAPACITY, acknowledge_painted_frame,
    try_collect_dirty_regions_into,
};
pub use damage::{LastDirtyRegions, collect_dirty_region, collect_dirty_regions, seed_prev_rects};
#[cfg(test)]
use damage::{needs_geometry_preparation, prepare_dirty_rects, quad_bbox, visible_paint_coverage};
use damage::{resolve_widget_state, visual_bounds};
pub use layout_snapshot::try_update_layout;
pub(crate) use layout_snapshot::{LayoutSnapshot, prepare_input_geometry, write_computed_rects};
#[cfg(test)]
use layout_snapshot::{LayoutTreeChanges, refresh_layout_tree, refresh_stable_layout_tree};
#[cfg(test)]
use layout_snapshot::{
    collect_entities_preorder, compute_layout_snapshot, reset_paint_preparation,
};
#[cfg(test)]
pub(crate) use paint::ScopedRenderer;
use paint::prepare_offscreen;
pub(crate) use paint::{
    acknowledge_rendered_outputs, count_nodes, prepare_paint_requests, render_region_cached,
    render_region_in_batch, render_regions_cached,
};
#[cfg(test)]
use paint::{draw_output_subtree, rects_intersect};
pub use paint::{render, render_region};

#[cfg(test)]
pub(crate) use text_layout::apply_text_intrinsic;
pub use text_layout::{TextLayoutFailure, TextLayoutFailureKind, TextLayoutStage};
#[cfg(test)]
use text_layout::{
    prepare_input_layout, prepare_text_geometry, text_layout_owner, visible_text_owner,
};

pub(crate) use scroll::has_pending_scroll;

#[cfg(all(test, feature = "std"))]
#[path = "../../tests/unit/ui/render_system/clip_children_check.rs"]
mod clip_children_check;
#[cfg(test)]
#[path = "../../tests/unit/ui/render_system/coverage_tests.rs"]
mod coverage_tests;
#[cfg(all(test, feature = "std"))]
#[path = "../../tests/unit/ui/render_system/disabled_state_check.rs"]
mod disabled_state_check;
#[cfg(test)]
#[path = "../../tests/unit/ui/render_system/geometry_preparation_tests.rs"]
mod geometry_preparation_tests;
#[cfg(all(test, feature = "std"))]
#[path = "../../tests/unit/ui/render_system/hidden_check.rs"]
mod hidden_check;
#[cfg(test)]
#[path = "../../tests/unit/ui/render_system/layout_snapshot_reuse_check.rs"]
mod layout_snapshot_reuse_check;
#[cfg(test)]
#[path = "../../tests/unit/ui/render_system/local_layout_tests.rs"]
mod local_layout_tests;
#[cfg(test)]
#[path = "../../tests/unit/ui/render_system/native_target_tests.rs"]
mod native_target_tests;
#[cfg(all(test, feature = "std"))]
#[path = "../../tests/unit/ui/render_system/offscreen_render_check.rs"]
mod offscreen_render_check;
#[cfg(test)]
#[path = "../../tests/unit/ui/render_system/overflow_tests.rs"]
mod overflow_tests;
#[cfg(test)]
#[path = "../../tests/unit/ui/render_system/paint_request_tests.rs"]
mod paint_request_tests;
#[cfg(test)]
#[path = "../../tests/unit/ui/render_system/projective_transform_tests.rs"]
mod projective_transform_tests;
#[cfg(all(test, feature = "std"))]
#[path = "../../tests/unit/ui/render_system/scroll_blit_visual_check.rs"]
mod scroll_blit_visual_check;
#[cfg(all(test, feature = "std"))]
#[path = "../../tests/unit/ui/render_system/scroll_plan_check.rs"]
mod scroll_plan_check;
#[cfg(test)]
#[path = "../../tests/unit/ui/render_system/state_resolve_check.rs"]
mod state_resolve_check;
#[cfg(all(test, feature = "std"))]
#[path = "../../tests/unit/ui/render_system/text_layout_check.rs"]
mod text_layout_check;
#[cfg(test)]
#[path = "../../tests/unit/ui/render_system/text_layout_failure_tests.rs"]
mod text_layout_failure_tests;
#[cfg(test)]
#[path = "../../tests/unit/ui/render_system/transform_cache_tests.rs"]
mod transform_cache_tests;
