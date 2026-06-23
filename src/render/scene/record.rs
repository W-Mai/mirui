//! Record a live `DrawCommand` stream into owned `SceneOp`s.

use super::{ResourceRef, SceneOp};
use crate::render::command::DrawCommand;
use crate::render::font::Font;
use crate::render::texture::Texture;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordError {
    BadUtf8,
}

/// Maps a borrowed `Font` / `Texture` to the `ResourceRef` that will appear
/// in the persisted scene (table index or runtime token).
pub trait ResourceResolver {
    fn resolve_font(&mut self, font: &Font) -> ResourceRef;
    fn resolve_texture(&mut self, texture: &Texture<'_>) -> ResourceRef;
}

pub fn record_command(
    cmd: &DrawCommand,
    resolver: &mut dyn ResourceResolver,
) -> Result<SceneOp, RecordError> {
    Ok(match cmd {
        DrawCommand::Fill {
            area,
            transform,
            quad,
            color,
            radius,
            opa,
        } => SceneOp::FillRect {
            area: *area,
            transform: *transform,
            quad: *quad,
            color: *color,
            radius: *radius,
            opa: *opa,
        },
        DrawCommand::Border {
            area,
            transform,
            quad,
            color,
            width,
            radius,
            opa,
        } => SceneOp::Border {
            area: *area,
            transform: *transform,
            quad: *quad,
            color: *color,
            width: *width,
            radius: *radius,
            opa: *opa,
        },
        DrawCommand::Label {
            pos,
            transform,
            text,
            font,
            color,
            opa,
        } => {
            let s = core::str::from_utf8(text).map_err(|_| RecordError::BadUtf8)?;
            SceneOp::Label {
                font: resolver.resolve_font(font),
                pos: *pos,
                transform: *transform,
                color: *color,
                opa: *opa,
                text: alloc::string::String::from(s).into(),
            }
        }
        DrawCommand::Line {
            p1,
            p2,
            transform,
            color,
            width,
            opa,
        } => SceneOp::Line {
            p1: *p1,
            p2: *p2,
            transform: *transform,
            color: *color,
            width: *width,
            opa: *opa,
        },
        DrawCommand::Arc {
            center,
            transform,
            radius,
            start_angle,
            end_angle,
            color,
            width,
            opa,
        } => SceneOp::Arc {
            center: *center,
            transform: *transform,
            radius: *radius,
            start_angle: *start_angle,
            end_angle: *end_angle,
            color: *color,
            width: *width,
            opa: *opa,
        },
        DrawCommand::Blit {
            pos,
            size,
            transform,
            quad,
            texture,
        } => SceneOp::Blit {
            texture: resolver.resolve_texture(texture),
            pos: *pos,
            size: *size,
            transform: *transform,
            quad: *quad,
        },
        DrawCommand::FillPath {
            path,
            transform,
            color,
            opa,
        } => SceneOp::FillPath {
            path: path.cmds.clone().into(),
            transform: *transform,
            color: *color,
            opa: *opa,
            fill_rule: crate::render::raster::FillRule::EvenOdd,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::path::{Path, PathCmd};
    use crate::render::scene::codec::{decode_scene, encode_scene};
    use crate::types::{Color, Fixed, Point, Rect, Transform};
    use alloc::vec;

    struct PanicResolver;
    impl ResourceResolver for PanicResolver {
        fn resolve_font(&mut self, _: &Font) -> ResourceRef {
            unreachable!("no Label in this fixture")
        }
        fn resolve_texture(&mut self, _: &Texture<'_>) -> ResourceRef {
            unreachable!("no Blit in this fixture")
        }
    }

    fn red() -> Color {
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        }
    }

    #[test]
    fn record_encode_decode_is_lossless() {
        let mut warp = Transform::IDENTITY;
        warp.tx = Fixed::from_int(7);
        let path = {
            let mut p = Path::new();
            p.move_to(Point::ZERO).line_to(Point {
                x: Fixed::from_int(4),
                y: Fixed::from_int(4),
            });
            p.cmds.push(PathCmd::Close);
            p
        };
        let commands = [
            DrawCommand::Fill {
                area: Rect {
                    x: Fixed::ZERO,
                    y: Fixed::ZERO,
                    w: Fixed::from_int(8),
                    h: Fixed::from_int(8),
                },
                transform: warp,
                quad: None,
                color: red(),
                radius: Fixed::from_int(2),
                opa: 200,
            },
            DrawCommand::Line {
                p1: Point::ZERO,
                p2: Point {
                    x: Fixed::from_int(5),
                    y: Fixed::from_int(5),
                },
                transform: Transform::IDENTITY,
                color: red(),
                width: Fixed::from_int(1),
                opa: 255,
            },
            DrawCommand::FillPath {
                path: &path,
                transform: Transform::IDENTITY,
                color: red(),
                opa: 255,
            },
        ];

        let mut recorded = vec::Vec::new();
        for cmd in &commands {
            recorded.push(record_command(cmd, &mut PanicResolver).unwrap());
        }
        let bytes = encode_scene(&recorded).unwrap();
        let back = decode_scene(&bytes).unwrap();
        assert_eq!(back, recorded);
    }
}
