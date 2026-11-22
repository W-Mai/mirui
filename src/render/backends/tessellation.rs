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
#[path = "../../../tests/unit/render/backends/tessellation.rs"]
mod tests;
