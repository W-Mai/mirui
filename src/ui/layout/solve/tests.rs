use super::*;
use crate::types::{Fixed, Rect};
use crate::ui::layout::{LayoutStyle, Padding, compute_layout};

fn tree() -> LayoutNode {
    let mut root = LayoutNode::new(LayoutStyle {
        width: Dimension::px(120),
        height: Dimension::px(90),
        padding: Padding::all(9),
        ..Default::default()
    });
    root.entity = Some(Entity {
        id: 0,
        generation: 1,
    });
    let mut leaf = LayoutNode::new(LayoutStyle {
        width: Dimension::px(20),
        height: Dimension::px(10),
        position: Position::Absolute,
        ..Default::default()
    });
    leaf.entity = Some(Entity {
        id: 1,
        generation: 1,
    });
    root.children.push(leaf);
    root.children.push(LayoutNode::new(LayoutStyle {
        width: Dimension::px(30),
        height: Dimension::px(20),
        ..Default::default()
    }));
    compute_layout(&mut root, Fixed::ZERO, Fixed::ZERO, 120.into(), 90.into());
    root
}

#[test]
fn admitted_leaf_uses_the_complete_absolute_solver_without_allocating() {
    for width in [
        Dimension::px(7),
        Dimension::percent(50),
        Dimension::Content,
        Dimension::Auto,
    ] {
        for constraint in [
            Dimension::Auto,
            Dimension::px(15),
            Dimension::Content,
            Dimension::percent(25),
        ] {
            let mut local = tree();
            let mut full = tree();
            for root in [&mut local, &mut full] {
                let leaf = &mut root.children[0];
                leaf.style.width = width;
                leaf.style.min_width = constraint;
                leaf.style.max_height = Dimension::px(4);
                leaf.style.left = Dimension::percent(75);
                leaf.style.top = Dimension::px(-3);
                leaf.intrinsic_width = Some(13.into());
            }
            let mut scope = SolveScope::default();
            scope.observe(&local.children[0], true);
            assert!(matches!(scope, SolveScope::Leaf(_)));
            assert_eq!(
                crate::test_support::tracked_allocations(|| assert!(scope.apply(&mut local))),
                0
            );
            compute_layout(&mut full, Fixed::ZERO, Fixed::ZERO, 120.into(), 90.into());
            assert_eq!(local.rect, full.rect);
            for (a, b) in local.children.iter().zip(&full.children) {
                assert_eq!(a.rect, b.rect);
            }
            assert_eq!(
                local.children[0].rect.x,
                Fixed::from_int(90),
                "absolute position uses the border box, not padding"
            );
        }
    }
}

#[test]
fn uncertain_or_multiple_changes_never_downgrade_to_a_leaf() {
    let mut root = tree();
    let leaf = &mut root.children[0];
    let mut scope = SolveScope::default();
    scope.observe(leaf, false);
    assert_eq!(scope, SolveScope::Full);
    scope.observe(leaf, true);
    assert_eq!(scope, SolveScope::Full);
    for missing in [Dimension::Auto, Dimension::Content] {
        leaf.style.width = missing;
        let mut scope = SolveScope::default();
        scope.observe(leaf, true);
        assert_eq!(scope, SolveScope::Full);
    }
    leaf.style.width = Dimension::px(20);
    leaf.style.min_height = Dimension::Content;
    let mut scope = SolveScope::default();
    scope.observe(leaf, true);
    assert_eq!(scope, SolveScope::Full);
    leaf.style.min_height = Dimension::Auto;
    let mut scope = SolveScope::default();
    scope.observe(leaf, true);
    scope.observe(leaf, true);
    assert_eq!(scope, SolveScope::Full);
    let target = leaf.entity.unwrap();
    leaf.entity.as_mut().unwrap().generation += 1;
    assert!(!SolveScope::Leaf(target).apply(&mut root));
    assert!(!SolveScope::Leaf(root.entity.unwrap()).apply(&mut root));
    root.children[0]
        .children
        .push(LayoutNode::new(LayoutStyle::default()));
    let mut scope = SolveScope::default();
    scope.observe(&root.children[0], true);
    assert_eq!(scope, SolveScope::Full);
    assert_ne!(root.rect, Rect::ZERO);
}
