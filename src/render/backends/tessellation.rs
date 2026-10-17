use lyon::math::Point as LyonPoint;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule as LyonFillRule, FillTessellator as LyonFillTessellator,
    FillVertex, GeometryBuilderError, LineCap as LyonLineCap, LineJoin as LyonLineJoin,
    StrokeOptions, StrokeTessellator as LyonStrokeTessellator, StrokeVertex, TessellationError,
    VertexBuffers,
};

use crate::render::raster::{FillRule, LineCap, LineJoin, StrokeSpec};
use crate::render::renderer::{RenderError, RenderResource};
#[cfg(any(test, feature = "wgpu"))]
use crate::render::{backends::lyon_path::to_lyon_path, path::Path};
use crate::types::Fixed;
#[cfg(any(test, feature = "wgpu"))]
use crate::types::Transform;

#[cfg(feature = "sdl-gpu")]
pub(super) mod geometry;

pub(super) struct FillTessellator {
    tessellator: LyonFillTessellator,
    buffers: VertexBuffers<LyonPoint, u32>,
}

impl FillTessellator {
    pub(super) fn new() -> Self {
        Self {
            tessellator: LyonFillTessellator::new(),
            buffers: VertexBuffers::new(),
        }
    }

    #[cfg(any(test, feature = "wgpu"))]
    pub(super) fn fill(
        &mut self,
        path: &Path,
        transform: Option<&Transform>,
        fill_rule: FillRule,
    ) -> Result<(&[LyonPoint], &[u32]), RenderError> {
        let path = to_lyon_path(path, transform);
        self.fill_lyon(&path, fill_rule)
    }

    pub(super) fn fill_lyon(
        &mut self,
        path: &lyon::path::Path,
        fill_rule: FillRule,
    ) -> Result<(&[LyonPoint], &[u32]), RenderError> {
        self.buffers.vertices.clear();
        self.buffers.indices.clear();
        let result = self.tessellator.tessellate_path(
            path,
            &FillOptions::tolerance(TOLERANCE).with_fill_rule(match fill_rule {
                FillRule::EvenOdd => LyonFillRule::EvenOdd,
                FillRule::NonZero => LyonFillRule::NonZero,
            }),
            &mut BuffersBuilder::new(&mut self.buffers, |vertex: FillVertex<'_>| {
                vertex.position()
            }),
        );
        finish_mesh(&mut self.buffers, result)
    }

    pub(super) fn take_mesh(&mut self) -> VertexBuffers<LyonPoint, u32> {
        core::mem::replace(&mut self.buffers, VertexBuffers::new())
    }

    pub(super) fn restore_mesh(&mut self, mesh: VertexBuffers<LyonPoint, u32>) {
        self.buffers = mesh;
    }
}

impl Default for FillTessellator {
    fn default() -> Self {
        Self::new()
    }
}

pub(super) struct StrokeTessellator {
    tessellator: LyonStrokeTessellator,
    buffers: VertexBuffers<LyonPoint, u32>,
}

impl StrokeTessellator {
    pub(super) fn new() -> Self {
        Self {
            tessellator: LyonStrokeTessellator::new(),
            buffers: VertexBuffers::new(),
        }
    }

    #[cfg(any(test, feature = "wgpu"))]
    pub(super) fn stroke(
        &mut self,
        path: &Path,
        transform: Option<&Transform>,
        spec: StrokeSpec<'_>,
    ) -> Result<(&[LyonPoint], &[u32]), RenderError> {
        let path = to_lyon_path(path, transform);
        self.stroke_lyon(&path, spec)
    }

    pub(super) fn stroke_lyon(
        &mut self,
        path: &lyon::path::Path,
        spec: StrokeSpec<'_>,
    ) -> Result<(&[LyonPoint], &[u32]), RenderError> {
        self.buffers.vertices.clear();
        self.buffers.indices.clear();
        let options = StrokeOptions::tolerance(TOLERANCE)
            .with_line_width(spec.width.to_f32())
            .with_line_cap(match spec.cap {
                LineCap::Butt => LyonLineCap::Butt,
                LineCap::Round => LyonLineCap::Round,
                LineCap::Square => LyonLineCap::Square,
            })
            .with_line_join(match spec.join {
                LineJoin::Miter => LyonLineJoin::Miter,
                LineJoin::Round => LyonLineJoin::Round,
                LineJoin::Bevel => LyonLineJoin::Bevel,
            })
            .with_miter_limit(spec.miter_limit.max(Fixed::ONE).to_f32());
        let result = self.tessellator.tessellate_path(
            path,
            &options,
            &mut BuffersBuilder::new(&mut self.buffers, |vertex: StrokeVertex<'_, '_>| {
                vertex.position()
            }),
        );
        finish_mesh(&mut self.buffers, result)
    }

    pub(super) fn take_mesh(&mut self) -> VertexBuffers<LyonPoint, u32> {
        core::mem::replace(&mut self.buffers, VertexBuffers::new())
    }

    pub(super) fn restore_mesh(&mut self, mesh: VertexBuffers<LyonPoint, u32>) {
        self.buffers = mesh;
    }
}

impl Default for StrokeTessellator {
    fn default() -> Self {
        Self::new()
    }
}

const TOLERANCE: f32 = 0.1;

fn finish_mesh(
    buffers: &mut VertexBuffers<LyonPoint, u32>,
    result: Result<(), TessellationError>,
) -> Result<(&[LyonPoint], &[u32]), RenderError> {
    if let Err(error) = result {
        buffers.vertices.clear();
        buffers.indices.clear();
        return Err(match error {
            TessellationError::GeometryBuilder(GeometryBuilderError::TooManyVertices) => {
                RenderError::ResourceLimit(RenderResource::Geometry)
            }
            TessellationError::GeometryBuilder(GeometryBuilderError::InvalidVertex)
            | TessellationError::UnsupportedParamater(_) => RenderError::InvalidGeometry,
            TessellationError::Internal(_) => RenderError::BackendFailure,
        });
    }
    Ok((&buffers.vertices, &buffers.indices))
}

#[cfg(test)]
mod tests {
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
}
