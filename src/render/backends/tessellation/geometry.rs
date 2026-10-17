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
mod tests {
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
}
