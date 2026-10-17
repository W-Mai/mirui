use lyon::math::{Point as LyonPoint, point as lyon_point};
use lyon::path::Path as LyonPath;

#[cfg(any(test, feature = "wgpu"))]
use crate::render::path::Path;
use crate::render::path::PathCmd;
use crate::types::Transform;

#[cfg(any(test, feature = "wgpu"))]
pub(super) fn to_lyon_path(path: &Path, transform: Option<&Transform>) -> LyonPath {
    commands_to_lyon(path.cmds.iter().cloned(), transform)
}

pub(super) fn commands_to_lyon(
    commands: impl IntoIterator<Item = PathCmd>,
    transform: Option<&Transform>,
) -> LyonPath {
    let mut builder = LyonPath::builder();
    let mut subpath_open = false;
    let point = |value: crate::types::Point| -> LyonPoint {
        let value = transform.map_or(value, |transform| transform.apply_point(value));
        lyon_point(value.x.to_f32(), value.y.to_f32())
    };

    for command in commands {
        match command {
            PathCmd::MoveTo(value) => {
                if subpath_open {
                    builder.end(false);
                }
                builder.begin(point(value));
                subpath_open = true;
            }
            PathCmd::LineTo(value) => {
                if !subpath_open {
                    builder.begin(point(value));
                    subpath_open = true;
                } else {
                    builder.line_to(point(value));
                }
            }
            PathCmd::QuadTo { ctrl, end } => {
                if !subpath_open {
                    builder.begin(point(end));
                    subpath_open = true;
                } else {
                    builder.quadratic_bezier_to(point(ctrl), point(end));
                }
            }
            PathCmd::CubicTo { ctrl1, ctrl2, end } => {
                if !subpath_open {
                    builder.begin(point(end));
                    subpath_open = true;
                } else {
                    builder.cubic_bezier_to(point(ctrl1), point(ctrl2), point(end));
                }
            }
            PathCmd::Close => {
                if subpath_open {
                    builder.end(true);
                    subpath_open = false;
                }
            }
        }
    }
    if subpath_open {
        builder.end(false);
    }
    builder.build()
}
