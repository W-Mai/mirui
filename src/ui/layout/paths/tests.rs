use super::*;
use crate::ui::layout::LayoutStyle;

fn tree() -> LayoutNode {
    let mut root = LayoutNode::new(LayoutStyle::default());
    root.entity = Some(Entity {
        id: 0,
        generation: 0,
    });
    for id in 1..65 {
        let mut child = LayoutNode::new(LayoutStyle::default());
        child.entity = Some(Entity { id, generation: 0 });
        root.children.push(child);
    }
    root
}

#[test]
fn sparse_paths_reuse_storage_and_revalidate_full_identity() {
    let mut tree = tree();
    let target = tree.children[20].entity.unwrap();
    let mut paths = LayoutPaths::default();
    paths.prepare(&tree, 65, [0; 3], |entity| entity == target);
    assert_eq!(paths.path(target), Some(&[20][..]));
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for _ in 0..32 {
                paths.prepare(&tree, 65, [0; 3], |_| panic!("unchanged demand"));
                assert_eq!(
                    paths.node_mut(&mut tree, target).unwrap().entity,
                    Some(target)
                );
            }
        }),
        0
    );
    tree.children.swap(20, 21);
    paths.invalidate();
    paths.prepare(&tree, 65, [0; 3], |entity| entity == target);
    assert_eq!(paths.path(target), Some(&[21][..]));
    tree.children[21].entity.as_mut().unwrap().generation += 1;
    assert!(paths.node_mut(&mut tree, target).is_none());
    paths.invalidate();
    paths.prepare(&tree, 65, [0; 3], |entity| entity == target);
    assert!(paths.path(target).is_none());
    assert_eq!(paths.entries.capacity(), 0);
    assert_eq!(paths.steps.capacity(), 0);
}

#[test]
fn dense_and_empty_choices_do_not_retry_until_invalidated() {
    let tree = tree();
    let mut paths = LayoutPaths::default();
    paths.prepare(&tree, 65, [0; 3], |_| true);
    assert!(paths.selected().is_none());
    assert_eq!(paths.entries.capacity(), 0);
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            paths.prepare(&tree, 65, [0; 3], |_| panic!("dense choice was prepared"));
        }),
        0
    );
    paths.prepare(&tree, 65, [1, 0, 0], |_| false);
    paths.prepare(&tree, 65, [1, 0, 0], |_| {
        panic!("empty choice was prepared")
    });
    paths.release();
    paths.prepare(&tree, 65, [1, 0, 0], |entity| entity.id == 1);
    assert!(paths.selected().is_some());
}

#[test]
fn shared_prefixes_are_visited_once_and_capacity_shrinks_on_demand_change() {
    let mut tree = tree();
    let mut branch = LayoutNode::new(LayoutStyle::default());
    branch.entity = Some(Entity {
        id: 100,
        generation: 0,
    });
    branch.children = core::mem::take(&mut tree.children);
    tree.children.push(branch);
    let mut paths = LayoutPaths::default();
    paths.prepare(&tree, 66, [0; 3], |entity| entity.id < 16);
    let selection = paths.selected().unwrap();
    let children: Vec<_> = selection.children(0).collect();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].0, 0);
    assert_eq!(children[0].1.children(1).count(), 15);
    paths.prepare(&tree, 66, [1, 0, 0], |entity| entity.id == 1);
    assert!(paths.entries.capacity() <= 4);
    assert!(paths.steps.capacity() <= 8);
    assert_eq!(
        paths.path(Entity {
            id: 1,
            generation: 0
        }),
        Some(&[0, 0][..])
    );
}
