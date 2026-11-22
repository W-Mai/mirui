use super::*;
use crate::types::Point;

#[test]
fn nested_contours_follow_requested_fill_rule() {
    let mut path = Path::new();
    for (lo, hi) in [(0, 10), (2, 8)] {
        path.move_to(Point::new(lo, lo))
            .line_to(Point::new(hi, lo))
            .line_to(Point::new(hi, hi))
            .line_to(Point::new(lo, hi))
            .close();
    }
    let mut tessellator = FillTessellator::new();
    let mut area = |rule| {
        let (vertices, indices) = tessellator.fill(&path, None, rule).unwrap();
        indices
            .chunks_exact(3)
            .map(|triangle| {
                let [a, b, c] = [
                    vertices[triangle[0] as usize],
                    vertices[triangle[1] as usize],
                    vertices[triangle[2] as usize],
                ];
                ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)).abs() / 2.0
            })
            .sum::<f32>()
    };
    assert!((area(FillRule::EvenOdd) - 64.0).abs() < 0.01);
    assert!((area(FillRule::NonZero) - 100.0).abs() < 0.01);
}

#[test]
fn curved_round_stroke_is_one_continuous_mesh() {
    let mut path = Path::new();
    path.move_to(Point::new(0, 0)).cubic_to(
        Point::new(30, 60),
        Point::new(70, -60),
        Point::new(100, 0),
    );
    let mut tessellator = StrokeTessellator::new();
    let (vertices, indices) = tessellator
        .stroke(
            &path,
            None,
            StrokeSpec {
                width: Fixed::from_int(6),
                cap: LineCap::Round,
                join: LineJoin::Round,
                miter_limit: Fixed::from_int(4),
                dash: &[],
                dash_scale: Fixed::ONE,
            },
        )
        .unwrap();

    assert!(vertices.len() > 8);
    assert!(indices.len() > 12);
    assert_eq!(indices.len() % 3, 0);
    assert!(
        indices
            .iter()
            .all(|index| (*index as usize) < vertices.len())
    );
}

#[test]
fn failed_mesh_is_discarded_without_losing_reusable_capacity() {
    use lyon::tessellation::{InternalError, UnsupportedParamater};

    let mut buffers = VertexBuffers::new();
    buffers.vertices.reserve(3);
    buffers.indices.reserve(3);
    let capacities = (buffers.vertices.capacity(), buffers.indices.capacity());
    for (error, expected) in [
        (
            GeometryBuilderError::InvalidVertex.into(),
            RenderError::InvalidGeometry,
        ),
        (
            GeometryBuilderError::TooManyVertices.into(),
            RenderError::ResourceLimit(RenderResource::Geometry),
        ),
        (
            TessellationError::UnsupportedParamater(UnsupportedParamater::PositionIsNaN),
            RenderError::InvalidGeometry,
        ),
        (
            InternalError::InsufficientNumberOfEdges.into(),
            RenderError::BackendFailure,
        ),
    ] {
        buffers.vertices.extend([LyonPoint::new(0.0, 0.0); 3]);
        buffers.indices.extend([0, 1, 2]);
        let allocations = crate::test_support::tracked_allocations(|| {
            assert_eq!(finish_mesh(&mut buffers, Err(error)), Err(expected));
        });
        assert_eq!(allocations, 0);
        assert!(buffers.vertices.is_empty());
        assert!(buffers.indices.is_empty());
        assert_eq!(
            (buffers.vertices.capacity(), buffers.indices.capacity()),
            capacities
        );
    }
    buffers.vertices.push(LyonPoint::new(2.0, 3.0));
    buffers.indices.push(0);
    let (vertices, indices) = finish_mesh(&mut buffers, Ok(())).unwrap();
    assert_eq!(vertices, &[LyonPoint::new(2.0, 3.0)]);
    assert_eq!(indices, &[0]);
}
