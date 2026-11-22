use super::*;

fn triangle(size: i32) -> Path {
    let mut path = Path::new();
    path.move_to(Point::new(0, 0))
        .line_to(Point::new(size, 0))
        .line_to(Point::new(0, size))
        .close();
    path
}

fn mesh() -> VertexBuffers<LyonPoint, u32> {
    VertexBuffers {
        vertices: alloc::vec![lyon::math::point(0.0, 0.0)],
        indices: alloc::vec![0],
    }
}

#[test]
fn normalized_lookup_is_exact_and_allocation_free() {
    let path = triangle(10);
    let request =
        ShapeRequest::new(path.commands(), None, ShapeStyle::Fill(FillRule::NonZero)).unwrap();
    let mut cache = GeometryCache::new(4096);
    cache.retain(&request, &mesh());
    let translate = Transform {
        tx: Fixed::from_ratio(101, 2),
        ty: Fixed::from_int(-17),
        ..Transform::IDENTITY
    };
    let moved = ShapeRequest::new(
        path.commands(),
        Some(&translate),
        ShapeStyle::Fill(FillRule::NonZero),
    )
    .unwrap();
    assert_eq!(moved.key, request.key);
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for _ in 0..16 {
                assert!(cache.find(&moved).is_some());
            }
        }),
        0
    );
    let other = triangle(11);
    let mut collision =
        ShapeRequest::new(other.commands(), None, ShapeStyle::Fill(FillRule::NonZero)).unwrap();
    collision.key = request.key;
    assert!(cache.find(&collision).is_none());
    assert!(cache.entries.is_empty());
}

#[test]
fn capacity_counts_signature_and_mesh_and_zero_disables_retention() {
    let path = triangle(10);
    let request =
        ShapeRequest::new(path.commands(), None, ShapeStyle::Fill(FillRule::NonZero)).unwrap();
    for budget in [0, 1, 4096] {
        let mut cache = GeometryCache::new(budget);
        cache.retain(&request, &mesh());
        assert!(cache.entries.current_size() <= budget);
        assert_eq!(cache.find(&request).is_some(), budget == 4096);
    }
}

#[test]
fn fill_rule_scale_and_stroke_semantics_change_signature() {
    use crate::render::raster::{LineCap, LineJoin};
    let path = triangle(10);
    let signature = |style, transform| {
        ShapeRequest::new(path.commands(), transform, style)
            .unwrap()
            .key
    };
    let fill = ShapeStyle::Fill(FillRule::NonZero);
    assert_ne!(
        signature(fill, None),
        signature(ShapeStyle::Fill(FillRule::EvenOdd), None)
    );
    let scaled = Transform {
        m00: Fixed::from_int(2),
        ..Transform::IDENTITY
    };
    assert_ne!(signature(fill, None), signature(fill, Some(&scaled)));
    let spec = StrokeSpec {
        width: Fixed::ONE,
        cap: LineCap::Butt,
        join: LineJoin::Miter,
        miter_limit: Fixed::from_int(4),
        dash: &[],
        dash_scale: Fixed::ONE,
    };
    let original = signature(ShapeStyle::Stroke(spec), None);
    for changed in [
        StrokeSpec {
            width: Fixed::from_int(2),
            ..spec
        },
        StrokeSpec {
            cap: LineCap::Round,
            ..spec
        },
        StrokeSpec {
            join: LineJoin::Bevel,
            ..spec
        },
        StrokeSpec {
            miter_limit: Fixed::from_int(3),
            ..spec
        },
        StrokeSpec {
            dash: &[Fixed::ONE],
            ..spec
        },
        StrokeSpec {
            dash_scale: Fixed::from_int(2),
            ..spec
        },
    ] {
        assert_ne!(original, signature(ShapeStyle::Stroke(changed), None));
    }
}

#[test]
fn normalization_rejects_unrepresentable_difference() {
    let mut path = Path::new();
    path.move_to(Point {
        x: Fixed::MIN,
        y: Fixed::ZERO,
    })
    .line_to(Point {
        x: Fixed::MAX,
        y: Fixed::ONE,
    });
    assert!(matches!(
        ShapeRequest::new(path.commands(), None, ShapeStyle::Fill(FillRule::NonZero)),
        Err(RenderError::InvalidGeometry)
    ));
}
