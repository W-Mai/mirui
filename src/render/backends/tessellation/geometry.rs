use alloc::vec::Vec;
use core::hash::{Hash, Hasher};

use lyon::{math::Point as LyonPoint, tessellation::VertexBuffers};

use crate::core::cache::{CacheInspect, Handle, HasSize, LruCache, MaxSize};
#[cfg(test)]
use crate::render::path::Path;
use crate::render::path::PathCmd;
use crate::render::raster::{FillRule, StrokeSpec};
use crate::render::renderer::RenderError;
use crate::types::{Fixed, Fixed64, Point, Transform};

#[derive(Clone, Copy)]
pub(crate) enum ShapeStyle<'a> {
    Fill(FillRule),
    Stroke(StrokeSpec<'a>),
}

impl ShapeStyle<'_> {
    fn words(self) -> [u64; 7] {
        let fixed = |v| crate::types::fixed::storage::to_i32(v) as u64;
        match self {
            Self::Fill(rule) => [0, rule as u64, 0, 0, 0, 0, 0],
            Self::Stroke(spec) => [
                1,
                fixed(spec.width),
                spec.cap as u64,
                spec.join as u64,
                fixed(spec.miter_limit.max(Fixed::ONE)),
                fixed(spec.dash_scale),
                spec.dash.len() as u64,
            ],
        }
    }
}

pub(crate) struct ShapeRequest<'a> {
    path: &'a [PathCmd],
    transform: Option<Transform>,
    pub origin: Point,
    style: ShapeStyle<'a>,
    key: u64,
}

impl<'a> ShapeRequest<'a> {
    pub fn new(
        path: &'a [PathCmd],
        transform: Option<&Transform>,
        style: ShapeStyle<'a>,
    ) -> Result<Self, RenderError> {
        let first = path.iter().find_map(|cmd| match cmd {
            PathCmd::MoveTo(p) | PathCmd::LineTo(p) => Some(*p),
            PathCmd::QuadTo { end, .. } | PathCmd::CubicTo { end, .. } => Some(*end),
            PathCmd::Close => None,
        });
        let origin = first.map_or(Point::ZERO, |p| transform.map_or(p, |tf| tf.apply_point(p)));
        Self::at_origin(path, transform, style, origin)
    }

    pub fn at_origin(
        path: &'a [PathCmd],
        transform: Option<&Transform>,
        style: ShapeStyle<'a>,
        origin: Point,
    ) -> Result<Self, RenderError> {
        let mut request = Self {
            path,
            transform: transform.copied(),
            origin,
            style,
            key: 0,
        };
        let mut hash = rustc_hash::FxHasher::default();
        style.words().hash(&mut hash);
        if let ShapeStyle::Stroke(spec) = style {
            spec.dash.hash(&mut hash);
        }
        for cmd in path {
            hash_command(&request.map(cmd)?, &mut hash);
        }
        request.key = hash.finish();
        Ok(request)
    }

    fn map(&self, cmd: &PathCmd) -> Result<PathCmd, RenderError> {
        let point = |p| -> Result<Point, RenderError> {
            let p = self.transform.map_or(p, |tf| tf.apply_point(p));
            let subtract = |a: Fixed, b: Fixed| {
                let delta = Fixed64::from(a) - Fixed64::from(b);
                if delta < Fixed64::from(Fixed::MIN) || delta > Fixed64::from(Fixed::MAX) {
                    Err(RenderError::InvalidGeometry)
                } else {
                    Ok(Fixed::from(delta))
                }
            };
            Ok(Point {
                x: subtract(p.x, self.origin.x)?,
                y: subtract(p.y, self.origin.y)?,
            })
        };
        Ok(match cmd {
            PathCmd::MoveTo(p) => PathCmd::MoveTo(point(*p)?),
            PathCmd::LineTo(p) => PathCmd::LineTo(point(*p)?),
            PathCmd::QuadTo { ctrl, end } => PathCmd::QuadTo {
                ctrl: point(*ctrl)?,
                end: point(*end)?,
            },
            PathCmd::CubicTo { ctrl1, ctrl2, end } => PathCmd::CubicTo {
                ctrl1: point(*ctrl1)?,
                ctrl2: point(*ctrl2)?,
                end: point(*end)?,
            },
            PathCmd::Close => PathCmd::Close,
        })
    }

    pub fn commands(&self) -> impl Iterator<Item = PathCmd> + '_ {
        self.path
            .iter()
            .map(|cmd| self.map(cmd).expect("validated normalized path"))
    }
}

fn hash_command(cmd: &PathCmd, hash: &mut impl Hasher) {
    let mut point = |tag: u8, p: Point| {
        tag.hash(hash);
        p.x.hash(hash);
        p.y.hash(hash);
    };
    match cmd {
        PathCmd::MoveTo(p) => point(0, *p),
        PathCmd::LineTo(p) => point(1, *p),
        PathCmd::QuadTo { ctrl, end } => {
            point(2, *ctrl);
            point(3, *end);
        }
        PathCmd::CubicTo { ctrl1, ctrl2, end } => {
            point(4, *ctrl1);
            point(5, *ctrl2);
            point(6, *end);
        }
        PathCmd::Close => 7_u8.hash(hash),
    }
}

pub(crate) struct ShapeMesh {
    commands: Vec<PathCmd>,
    style: [u64; 7],
    dash: Vec<Fixed>,
    pub mesh: VertexBuffers<LyonPoint, u32>,
}

impl HasSize for ShapeMesh {
    fn cache_size(&self) -> usize {
        self.commands.capacity() * size_of::<PathCmd>()
            + self.dash.capacity() * size_of::<Fixed>()
            + self.mesh.vertices.capacity() * size_of::<LyonPoint>()
            + self.mesh.indices.capacity() * size_of::<u32>()
            + size_of::<Self>()
    }
}

pub(crate) struct GeometryCache {
    entries: LruCache<u64, ShapeMesh>,
    pub budget: usize,
}

impl GeometryCache {
    pub fn new(budget: usize) -> Self {
        Self {
            entries: LruCache::builder()
                .max_size(MaxSize::Bytes(budget))
                .name("sdl_gpu/geometry")
                .build(),
            budget,
        }
    }

    pub fn inspect(&self) -> &dyn CacheInspect {
        &self.entries
    }

    pub fn find(&mut self, request: &ShapeRequest<'_>) -> Option<Handle<ShapeMesh>> {
        if self.budget == 0 {
            return None;
        }
        let handle = self.entries.acquire(&request.key)?;
        let item = handle.get();
        let dash = match request.style {
            ShapeStyle::Fill(_) => &[][..],
            ShapeStyle::Stroke(spec) => spec.dash,
        };
        if item.style == request.style.words()
            && item.dash == dash
            && item.commands.iter().cloned().eq(request.commands())
        {
            Some(handle)
        } else {
            self.entries.drop(&request.key);
            None
        }
    }

    pub fn retain(&mut self, request: &ShapeRequest<'_>, mesh: &VertexBuffers<LyonPoint, u32>) {
        let dash = match request.style {
            ShapeStyle::Fill(_) => &[][..],
            ShapeStyle::Stroke(spec) => spec.dash,
        };
        let bytes = size_of::<ShapeMesh>()
            + size_of_val(request.path)
            + size_of_val(dash)
            + mesh.vertices.len() * size_of::<LyonPoint>()
            + mesh.indices.len() * size_of::<u32>();
        if bytes > self.budget {
            return;
        }
        let entry = ShapeMesh {
            commands: request.commands().collect(),
            style: request.style.words(),
            dash: dash.to_vec(),
            mesh: mesh.clone(),
        };
        if entry.cache_size() <= self.budget {
            self.entries.entry(request.key).or_insert_with(|| entry);
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/render/backends/tessellation/geometry.rs"]
mod tests;
