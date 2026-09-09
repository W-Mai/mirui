#[cfg(test)]
mod tests {
    use mirui::types::{Dimension, Fixed, Rect};
    use mirui::ui::layout::*;

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
}
