#[cfg(test)]
mod tests {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    use mirui::types::{Dimension, Fixed, Rect};
    use mirui::ui::layout::*;

    struct TrackingAllocator;

    std::thread_local! {
        static TRACK_ALLOCATIONS: Cell<bool> = const { Cell::new(false) };
        static ALLOCATION_COUNT: Cell<usize> = const { Cell::new(0) };
    }

    unsafe impl GlobalAlloc for TrackingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            TRACK_ALLOCATIONS.with(|tracking| {
                if tracking.get() {
                    ALLOCATION_COUNT.with(|count| count.set(count.get() + 1));
                }
            });
            // SAFETY: `layout` is forwarded unchanged to the system allocator.
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // SAFETY: `ptr` and `layout` came from this allocator's system allocation.
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            TRACK_ALLOCATIONS.with(|tracking| {
                if tracking.get() {
                    ALLOCATION_COUNT.with(|count| count.set(count.get() + 1));
                }
            });
            // SAFETY: the allocation remains owned by the system allocator.
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: TrackingAllocator = TrackingAllocator;

    fn tracked_allocations(run: impl FnOnce()) -> usize {
        ALLOCATION_COUNT.with(|count| count.set(0));
        TRACK_ALLOCATIONS.with(|tracking| tracking.set(true));
        run();
        TRACK_ALLOCATIONS.with(|tracking| tracking.set(false));
        ALLOCATION_COUNT.with(Cell::get)
    }

    #[test]
    fn row_fixed_sizes() {
        let mut root = LayoutNode::new(LayoutStyle {
            direction: FlexDirection::Row,
            width: Dimension::px(300),
            height: Dimension::px(100),
            ..Default::default()
        });
        root.add_child(LayoutNode::new(LayoutStyle {
            width: Dimension::px(100),
            height: Dimension::px(50),
            ..Default::default()
        }));
        root.add_child(LayoutNode::new(LayoutStyle {
            width: Dimension::px(100),
            height: Dimension::px(50),
            ..Default::default()
        }));

        compute_layout(
            &mut root,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(300),
            Fixed::from_int(100),
        );

        assert_eq!(
            root.children[0].rect,
            Rect {
                x: Fixed::from_int(0),
                y: Fixed::from_int(0),
                w: Fixed::from_int(100),
                h: Fixed::from_int(50)
            }
        );
        assert_eq!(
            root.children[1].rect,
            Rect {
                x: Fixed::from_int(100),
                y: Fixed::from_int(0),
                w: Fixed::from_int(100),
                h: Fixed::from_int(50)
            }
        );
    }

    #[test]
    fn column_fixed_sizes() {
        let mut root = LayoutNode::new(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(100),
            height: Dimension::px(200),
            ..Default::default()
        });
        root.add_child(LayoutNode::new(LayoutStyle {
            width: Dimension::px(80),
            height: Dimension::px(60),
            ..Default::default()
        }));
        root.add_child(LayoutNode::new(LayoutStyle {
            width: Dimension::px(80),
            height: Dimension::px(60),
            ..Default::default()
        }));

        compute_layout(
            &mut root,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(100),
            Fixed::from_int(200),
        );

        assert_eq!(
            root.children[0].rect,
            Rect {
                x: Fixed::from_int(0),
                y: Fixed::from_int(0),
                w: Fixed::from_int(80),
                h: Fixed::from_int(60)
            }
        );
        assert_eq!(
            root.children[1].rect,
            Rect {
                x: Fixed::from_int(0),
                y: Fixed::from_int(60),
                w: Fixed::from_int(80),
                h: Fixed::from_int(60)
            }
        );
    }

    #[test]
    fn row_space_between() {
        let mut root = LayoutNode::new(LayoutStyle {
            direction: FlexDirection::Row,
            justify: JustifyContent::SpaceBetween,
            width: Dimension::px(300),
            height: Dimension::px(100),
            ..Default::default()
        });
        root.add_child(LayoutNode::new(LayoutStyle {
            width: Dimension::px(50),
            height: Dimension::px(50),
            ..Default::default()
        }));
        root.add_child(LayoutNode::new(LayoutStyle {
            width: Dimension::px(50),
            height: Dimension::px(50),
            ..Default::default()
        }));
        root.add_child(LayoutNode::new(LayoutStyle {
            width: Dimension::px(50),
            height: Dimension::px(50),
            ..Default::default()
        }));

        compute_layout(
            &mut root,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(300),
            Fixed::from_int(100),
        );

        assert_eq!(root.children[0].rect.x, Fixed::from_int(0));
        assert_eq!(root.children[2].rect.x, Fixed::from_int(250)); // 300 - 50
        // middle should be centered: (300 - 150) / 2 = 75
        assert_eq!(root.children[1].rect.x, Fixed::from_int(125));
    }

    #[test]
    fn row_grow() {
        let mut root = LayoutNode::new(LayoutStyle {
            direction: FlexDirection::Row,
            width: Dimension::px(300),
            height: Dimension::px(100),
            ..Default::default()
        });
        root.add_child(LayoutNode::new(LayoutStyle {
            grow: Fixed::from_f32(1.0),
            height: Dimension::px(100),
            ..Default::default()
        }));
        root.add_child(LayoutNode::new(LayoutStyle {
            grow: Fixed::from_f32(2.0),
            height: Dimension::px(100),
            ..Default::default()
        }));

        compute_layout(
            &mut root,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(300),
            Fixed::from_int(100),
        );

        assert_eq!(root.children[0].rect.w, Fixed::from_int(100)); // 1/3 of 300
        assert_eq!(root.children[1].rect.w, Fixed::from_int(200)); // 2/3 of 300
    }

    #[test]
    fn align_center() {
        let mut root = LayoutNode::new(LayoutStyle {
            direction: FlexDirection::Row,
            align: AlignItems::Center,
            width: Dimension::px(200),
            height: Dimension::px(100),
            ..Default::default()
        });
        root.add_child(LayoutNode::new(LayoutStyle {
            width: Dimension::px(50),
            height: Dimension::px(30),
            ..Default::default()
        }));

        compute_layout(
            &mut root,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(200),
            Fixed::from_int(100),
        );

        // centered: (100 - 30) / 2 = 35
        assert_eq!(root.children[0].rect.y, Fixed::from_int(35));
    }

    #[test]
    fn padding() {
        let mut root = LayoutNode::new(LayoutStyle {
            direction: FlexDirection::Row,
            width: Dimension::px(200),
            height: Dimension::px(100),
            padding: Padding {
                top: 10.into(),
                right: 10.into(),
                bottom: 10.into(),
                left: 10.into(),
            },
            ..Default::default()
        });
        root.add_child(LayoutNode::new(LayoutStyle {
            width: Dimension::px(50),
            height: Dimension::px(50),
            ..Default::default()
        }));

        compute_layout(
            &mut root,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(200),
            Fixed::from_int(100),
        );

        assert_eq!(root.children[0].rect.x, Fixed::from_int(10));
        assert_eq!(root.children[0].rect.y, Fixed::from_int(10));
    }

    #[test]
    fn axis_gaps_reserve_space_before_grow() {
        let mut row = LayoutNode::new(LayoutStyle {
            direction: FlexDirection::Row,
            column_gap: Dimension::px(10),
            width: Dimension::px(100),
            height: Dimension::px(20),
            ..Default::default()
        });
        for _ in 0..2 {
            row.add_child(LayoutNode::new(LayoutStyle {
                grow: Fixed::ONE,
                ..Default::default()
            }));
        }

        compute_layout(
            &mut row,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(100),
            Fixed::from_int(20),
        );

        assert_eq!(row.children[0].rect.w, Fixed::from_int(45));
        assert_eq!(row.children[1].rect.x, Fixed::from_int(55));

        let mut column = LayoutNode::new(LayoutStyle {
            direction: FlexDirection::Column,
            row_gap: Dimension::px(6),
            width: Dimension::px(20),
            height: Dimension::px(50),
            ..Default::default()
        });
        for _ in 0..2 {
            column.add_child(LayoutNode::new(LayoutStyle {
                height: Dimension::px(12),
                ..Default::default()
            }));
        }

        compute_layout(
            &mut column,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(20),
            Fixed::from_int(50),
        );

        assert_eq!(column.children[1].rect.y, Fixed::from_int(18));
    }

    #[test]
    fn fixed_gap_and_distributed_space_compose() {
        let mut root = LayoutNode::new(LayoutStyle {
            justify: JustifyContent::SpaceBetween,
            column_gap: Dimension::px(10),
            width: Dimension::px(100),
            height: Dimension::px(20),
            ..Default::default()
        });
        for _ in 0..2 {
            root.add_child(LayoutNode::new(LayoutStyle {
                width: Dimension::px(20),
                ..Default::default()
            }));
        }

        compute_layout(
            &mut root,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(100),
            Fixed::from_int(20),
        );

        assert_eq!(root.children[1].rect.x, Fixed::from_int(80));
    }

    #[test]
    fn distributed_space_accepts_only_absolute_children() {
        let mut root = LayoutNode::new(LayoutStyle {
            justify: JustifyContent::SpaceAround,
            width: Dimension::px(100),
            height: Dimension::px(20),
            ..Default::default()
        });
        root.add_child(LayoutNode::new(LayoutStyle {
            position: Position::Absolute,
            width: Dimension::px(10),
            height: Dimension::px(10),
            ..Default::default()
        }));

        compute_layout(
            &mut root,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(100),
            Fixed::from_int(20),
        );

        assert_eq!(root.children[0].rect, Rect::new(0, 0, 10, 10));
    }

    #[test]
    fn min_and_max_constraints_clamp_fixed_and_root_sizes() {
        let mut root = LayoutNode::new(LayoutStyle {
            max_width: Dimension::px(80),
            height: Dimension::px(20),
            ..Default::default()
        });
        root.add_child(LayoutNode::new(LayoutStyle {
            width: Dimension::px(10),
            min_width: Dimension::px(30),
            height: Dimension::px(40),
            max_height: Dimension::px(16),
            ..Default::default()
        }));

        compute_layout(
            &mut root,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(100),
            Fixed::from_int(20),
        );

        assert_eq!(root.rect.w, Fixed::from_int(80));
        assert_eq!(root.children[0].rect.w, Fixed::from_int(30));
        assert_eq!(root.children[0].rect.h, Fixed::from_int(16));
    }

    #[test]
    fn constrained_grow_redistributes_remaining_space() {
        let mut max_root = LayoutNode::new(LayoutStyle {
            width: Dimension::px(100),
            height: Dimension::px(20),
            ..Default::default()
        });
        max_root.add_child(LayoutNode::new(LayoutStyle {
            grow: Fixed::ONE,
            max_width: Dimension::px(30),
            ..Default::default()
        }));
        max_root.add_child(LayoutNode::new(LayoutStyle {
            grow: Fixed::ONE,
            ..Default::default()
        }));

        compute_layout(
            &mut max_root,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(100),
            Fixed::from_int(20),
        );

        assert_eq!(max_root.children[0].rect.w, Fixed::from_int(30));
        assert_eq!(max_root.children[1].rect.w, Fixed::from_int(70));
        assert_eq!(max_root.children[1].rect.x, Fixed::from_int(30));

        let mut min_root = LayoutNode::new(LayoutStyle {
            width: Dimension::px(100),
            height: Dimension::px(20),
            ..Default::default()
        });
        min_root.add_child(LayoutNode::new(LayoutStyle {
            grow: Fixed::ONE,
            min_width: Dimension::px(60),
            ..Default::default()
        }));
        min_root.add_child(LayoutNode::new(LayoutStyle {
            grow: Fixed::ONE,
            ..Default::default()
        }));

        compute_layout(
            &mut min_root,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(100),
            Fixed::from_int(20),
        );

        assert_eq!(min_root.children[0].rect.w, Fixed::from_int(60));
        assert_eq!(min_root.children[1].rect.w, Fixed::from_int(40));
        assert_eq!(min_root.children[1].rect.x, Fixed::from_int(60));
    }

    #[test]
    fn minimum_wins_when_bounds_cross() {
        let mut root = LayoutNode::new(LayoutStyle {
            width: Dimension::px(100),
            height: Dimension::px(20),
            ..Default::default()
        });
        root.add_child(LayoutNode::new(LayoutStyle {
            width: Dimension::px(40),
            min_width: Dimension::px(60),
            max_width: Dimension::px(20),
            ..Default::default()
        }));

        compute_layout(
            &mut root,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(100),
            Fixed::from_int(20),
        );

        assert_eq!(root.children[0].rect.w, Fixed::from_int(60));
    }

    #[test]
    fn shrink_uses_scaled_base_sizes() {
        let mut root = LayoutNode::new(LayoutStyle {
            width: Dimension::px(60),
            height: Dimension::px(20),
            ..Default::default()
        });
        root.add_child(LayoutNode::new(LayoutStyle {
            width: Dimension::px(80),
            shrink: Fixed::ONE,
            ..Default::default()
        }));
        root.add_child(LayoutNode::new(LayoutStyle {
            width: Dimension::px(40),
            shrink: Fixed::ONE,
            ..Default::default()
        }));

        compute_layout(
            &mut root,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(60),
            Fixed::from_int(20),
        );

        assert_eq!(root.children[0].rect.w, Fixed::from_int(40));
        assert_eq!(root.children[1].rect.w, Fixed::from_int(20));
        assert_eq!(root.children[1].rect.x, Fixed::from_int(40));
    }

    #[test]
    fn shrink_redistributes_after_minimum_is_reached() {
        let mut root = LayoutNode::new(LayoutStyle {
            width: Dimension::px(100),
            height: Dimension::px(20),
            ..Default::default()
        });
        root.add_child(LayoutNode::new(LayoutStyle {
            width: Dimension::px(80),
            min_width: Dimension::px(70),
            shrink: Fixed::ONE,
            ..Default::default()
        }));
        root.add_child(LayoutNode::new(LayoutStyle {
            width: Dimension::px(80),
            shrink: Fixed::ONE,
            ..Default::default()
        }));

        compute_layout(
            &mut root,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(100),
            Fixed::from_int(20),
        );

        assert_eq!(root.children[0].rect.w, Fixed::from_int(70));
        assert_eq!(root.children[1].rect.w, Fixed::from_int(30));
        assert_eq!(root.children[1].rect.x, Fixed::from_int(70));
    }

    #[test]
    fn shrink_is_opt_in() {
        let mut root = LayoutNode::new(LayoutStyle {
            width: Dimension::px(100),
            height: Dimension::px(20),
            ..Default::default()
        });
        for _ in 0..2 {
            root.add_child(LayoutNode::new(LayoutStyle {
                width: Dimension::px(80),
                ..Default::default()
            }));
        }

        compute_layout(
            &mut root,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(100),
            Fixed::from_int(20),
        );

        assert_eq!(root.children[0].rect.w, Fixed::from_int(80));
        assert_eq!(root.children[1].rect.w, Fixed::from_int(80));
        assert_eq!(root.children[1].rect.x, Fixed::from_int(80));
    }

    #[test]
    fn row_wraps_with_independent_axis_gaps() {
        let mut root = LayoutNode::new(LayoutStyle {
            wrap: FlexWrap::Wrap,
            width: Dimension::px(100),
            height: Dimension::px(100),
            row_gap: Dimension::px(5),
            column_gap: Dimension::px(10),
            ..Default::default()
        });
        for _ in 0..3 {
            root.add_child(LayoutNode::new(LayoutStyle {
                width: Dimension::px(45),
                height: Dimension::px(20),
                ..Default::default()
            }));
        }

        compute_layout(
            &mut root,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(100),
            Fixed::from_int(100),
        );

        assert_eq!(root.children[0].rect, Rect::new(0, 0, 45, 20));
        assert_eq!(root.children[1].rect, Rect::new(55, 0, 45, 20));
        assert_eq!(root.children[2].rect, Rect::new(0, 25, 45, 20));
    }

    #[test]
    fn column_wraps_with_independent_axis_gaps() {
        let mut root = LayoutNode::new(LayoutStyle {
            direction: FlexDirection::Column,
            wrap: FlexWrap::Wrap,
            width: Dimension::px(100),
            height: Dimension::px(100),
            row_gap: Dimension::px(10),
            column_gap: Dimension::px(5),
            ..Default::default()
        });
        for _ in 0..3 {
            root.add_child(LayoutNode::new(LayoutStyle {
                width: Dimension::px(20),
                height: Dimension::px(45),
                ..Default::default()
            }));
        }

        compute_layout(
            &mut root,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(100),
            Fixed::from_int(100),
        );

        assert_eq!(root.children[0].rect, Rect::new(0, 0, 20, 45));
        assert_eq!(root.children[1].rect, Rect::new(0, 55, 20, 45));
        assert_eq!(root.children[2].rect, Rect::new(25, 0, 20, 45));
    }

    #[test]
    fn wrapped_lines_resolve_grow_and_shrink_independently() {
        let mut root = LayoutNode::new(LayoutStyle {
            wrap: FlexWrap::Wrap,
            width: Dimension::px(100),
            height: Dimension::px(100),
            row_gap: Dimension::px(5),
            column_gap: Dimension::px(10),
            ..Default::default()
        });
        root.add_child(LayoutNode::new(LayoutStyle {
            width: Dimension::px(60),
            height: Dimension::px(20),
            ..Default::default()
        }));
        root.add_child(LayoutNode::new(LayoutStyle {
            grow: Fixed::ONE,
            height: Dimension::px(20),
            ..Default::default()
        }));
        root.add_child(LayoutNode::new(LayoutStyle {
            width: Dimension::px(120),
            height: Dimension::px(20),
            shrink: Fixed::ONE,
            ..Default::default()
        }));

        compute_layout(
            &mut root,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(100),
            Fixed::from_int(100),
        );

        assert_eq!(root.children[0].rect, Rect::new(0, 0, 60, 20));
        assert_eq!(root.children[1].rect, Rect::new(70, 0, 30, 20));
        assert_eq!(root.children[2].rect, Rect::new(0, 25, 100, 20));
    }

    #[test]
    fn minimums_wrap_flexible_items_before_grow_distribution() {
        let mut root = LayoutNode::new(LayoutStyle {
            wrap: FlexWrap::Wrap,
            width: Dimension::px(100),
            height: Dimension::px(100),
            row_gap: Dimension::px(5),
            column_gap: Dimension::px(10),
            ..Default::default()
        });
        for _ in 0..2 {
            root.add_child(LayoutNode::new(LayoutStyle {
                min_width: Dimension::px(60),
                height: Dimension::px(20),
                grow: Fixed::ONE,
                ..Default::default()
            }));
        }

        compute_layout(
            &mut root,
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(100),
            Fixed::from_int(100),
        );

        assert_eq!(root.children[0].rect, Rect::new(0, 0, 100, 20));
        assert_eq!(root.children[1].rect, Rect::new(0, 25, 100, 20));
    }

    #[test]
    fn responsive_layout_reuses_existing_tree_storage() {
        let mut root = LayoutNode::new(LayoutStyle {
            wrap: FlexWrap::Wrap,
            width: Dimension::px(160),
            height: Dimension::px(120),
            row_gap: Dimension::px(6),
            column_gap: Dimension::px(8),
            ..Default::default()
        });
        for index in 0..12 {
            root.add_child(LayoutNode::new(LayoutStyle {
                width: Dimension::px(36 + index % 3 * 4),
                min_width: Dimension::px(24),
                max_width: Dimension::px(48),
                height: Dimension::px(18),
                shrink: Fixed::ONE,
                ..Default::default()
            }));
        }

        let allocations = tracked_allocations(|| {
            compute_layout(
                &mut root,
                Fixed::ZERO,
                Fixed::ZERO,
                Fixed::from_int(160),
                Fixed::from_int(120),
            );
        });

        assert_eq!(allocations, 0);
        assert_eq!(root.children[0].rect, Rect::new(0, 0, 36, 18));
        assert!(root.children[11].rect.y > Fixed::ZERO);
    }
}
