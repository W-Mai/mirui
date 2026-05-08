#[cfg(test)]
mod tests {
    use mirui::layout::*;
    use mirui::types::{Dimension, Fixed, Rect};

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

        compute_layout(&mut root, 0, 0, 300, 100);

        assert_eq!(
            root.children[0].rect,
            Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 50
            }
        );
        assert_eq!(
            root.children[1].rect,
            Rect {
                x: 100,
                y: 0,
                w: 100,
                h: 50
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

        compute_layout(&mut root, 0, 0, 100, 200);

        assert_eq!(
            root.children[0].rect,
            Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 60
            }
        );
        assert_eq!(
            root.children[1].rect,
            Rect {
                x: 0,
                y: 60,
                w: 80,
                h: 60
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

        compute_layout(&mut root, 0, 0, 300, 100);

        assert_eq!(root.children[0].rect.x, 0);
        assert_eq!(root.children[2].rect.x, 250); // 300 - 50
        // middle should be centered: (300 - 150) / 2 = 75
        assert_eq!(root.children[1].rect.x, 125);
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

        compute_layout(&mut root, 0, 0, 300, 100);

        assert_eq!(root.children[0].rect.w, 100); // 1/3 of 300
        assert_eq!(root.children[1].rect.w, 200); // 2/3 of 300
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

        compute_layout(&mut root, 0, 0, 200, 100);

        // centered: (100 - 30) / 2 = 35
        assert_eq!(root.children[0].rect.y, 35);
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

        compute_layout(&mut root, 0, 0, 200, 100);

        assert_eq!(root.children[0].rect.x, 10);
        assert_eq!(root.children[0].rect.y, 10);
    }
}
