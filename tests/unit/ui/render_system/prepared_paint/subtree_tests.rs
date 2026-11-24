use super::*;
use crate::ui::layout::{LayoutNode, LayoutStyle};

#[test]
fn spans_follow_structure_and_survive_paint_cache_resets() {
    let mut root = LayoutNode::new(LayoutStyle::default());
    let mut branch = LayoutNode::new(LayoutStyle::default());
    branch.add_child(LayoutNode::new(LayoutStyle::default()));
    branch.add_child(LayoutNode::new(LayoutStyle::default()));
    root.add_child(branch);
    root.add_child(LayoutNode::new(LayoutStyle::default()));
    let mut prepared = PreparedPaint::with_capacity(5);
    prepared.resize(5, 0);
    prepared.prepare_subtrees(&root, true);
    assert_eq!(
        (0..5).map(|i| prepared.subtree_end(i)).collect::<Vec<_>>(),
        [Some(5), Some(4), Some(3), Some(4), Some(5)]
    );
    prepared.clear();
    prepared.resize(5, 0);
    assert_eq!(prepared.subtree_end(1), Some(4));
    root.children.swap(0, 1);
    prepared.prepare_subtrees(&root, true);
    assert_eq!(prepared.subtree_end(1), Some(2));
    assert_eq!(prepared.subtree_end(2), Some(5));
    root.children.remove(1);
    prepared.resize(2, 0);
    prepared.prepare_subtrees(&root, true);
    assert_eq!(prepared.subtree_end(0), Some(2));
    assert_eq!(prepared.subtree_end(2), None);
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            prepared.prepare_subtrees(&root, false);
            prepared.clear_frame();
        }),
        0
    );
}
