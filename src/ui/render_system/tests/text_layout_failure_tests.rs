use super::{TextLayoutFailure, TextLayoutFailureKind, TextLayoutStage};
use crate::ecs::Entity;
use crate::text::baseline::PathBaselineError;

#[test]
fn path_baseline_capacity_diagnostics_keep_the_required_budget() {
    let entity = Entity {
        id: 1,
        generation: 0,
    };
    let failure = TextLayoutFailure::baseline(
        entity,
        TextLayoutStage::Layout,
        PathBaselineError::CacheBudget {
            required: 256,
            budget: 128,
        },
    );
    assert_eq!(
        failure.kind,
        TextLayoutFailureKind::PathBaselineBudget {
            required: 256,
            budget: 128,
        }
    );
    assert_eq!(
        TextLayoutFailure::baseline(
            entity,
            TextLayoutStage::Measure,
            PathBaselineError::Allocation,
        )
        .kind,
        TextLayoutFailureKind::PathBaselineAllocation,
    );
}
