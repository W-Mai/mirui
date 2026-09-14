//! Record a live `DrawCommand` stream into owned `SceneOp`s.

use super::{Paint, PosedGlyphBuffer, ResourceRef, SceneOp};
use crate::render::command::DrawCommand;
use crate::render::font::Font;
use crate::render::path::Path;
use crate::render::texture::Texture;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordError {
    BadUtf8,
    UnsupportedCommand,
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
        DrawCommand::GlyphRun {
            pos,
            transform,
            glyphs,
            font,
            color,
            opa,
        } => SceneOp::GlyphRun {
            font: resolver.resolve_font(font),
            ppem: font.size,
            pos: *pos,
            transform: *transform,
            color: *color,
            opa: *opa,
            glyphs: glyphs.to_vec().into(),
        },
        DrawCommand::PosedGlyphRun {
            pos,
            transform,
            glyphs,
            font,
            color,
            opa,
        } => SceneOp::PosedGlyphRun {
            font: resolver.resolve_font(font),
            ppem: font.size,
            pos: *pos,
            transform: *transform,
            color: *color,
            opa: *opa,
            glyphs: PosedGlyphBuffer::new(glyphs.glyphs().to_vec(), glyphs.frames().to_vec())
                .expect("draw glyph pose lengths are validated at construction"),
        },
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
            opa,
            radius,
            composite,
        } => SceneOp::Blit {
            texture: resolver.resolve_texture(texture),
            pos: *pos,
            size: *size,
            transform: *transform,
            quad: *quad,
            opa: *opa,
            radius: *radius,
            composite: *composite,
        },
        DrawCommand::FillPath {
            path,
            transform,
            paint,
            opa,
            fill_rule,
        } => SceneOp::FillPath {
            path: Path::clone(path),
            transform: *transform,
            paint: Paint::clone(paint),
            opa: *opa,
            fill_rule: *fill_rule,
        },
        DrawCommand::StrokePath {
            path,
            transform,
            paint,
            width,
            opa,
            line_cap,
            line_join,
            miter_limit,
            dash,
        } => SceneOp::StrokePath {
            path: Path::clone(path),
            transform: *transform,
            paint: Paint::clone(paint),
            width: *width,
            opa: *opa,
            line_cap: *line_cap,
            line_join: *line_join,
            miter_limit: *miter_limit,
            dash: alloc::borrow::Cow::Owned(dash.to_vec()),
        },
        DrawCommand::PushClip {
            path,
            transform,
            fill_rule,
        } => SceneOp::PushClip {
            path: Path::clone(path),
            transform: *transform,
            fill_rule: *fill_rule,
        },
        DrawCommand::PopClip => SceneOp::PopClip,
        DrawCommand::ApplyBlur { .. } => return Err(RecordError::UnsupportedCommand),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::font::GlyphId;
    use crate::render::path::{Path, PathCmd};
    use crate::render::scene::codec::{decode_scene, encode_scene};
    use crate::types::{Color, Fixed, Point, Rect, Transform};
    use alloc::vec;

    struct PanicResolver;
    impl ResourceResolver for PanicResolver {
        fn resolve_font(&mut self, _: &Font) -> ResourceRef {
            unreachable!("fixture has no font commands")
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
            p.cmds.to_mut().push(PathCmd::Close);
            p
        };
        let path_paint = Paint::Color(red().into());
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
                paint: &path_paint,
                opa: 255,
                fill_rule: crate::render::raster::FillRule::EvenOdd,
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

    #[test]
    fn positioned_glyphs_retain_their_placements() {
        let font = Font::bitmap_8x8();
        let glyphs = [textflow::shaping::PositionedGlyph::new(
            GlyphId::new(65),
            textflow::shaping::FlowPoint { x: 0, y: 7 << 8 },
        )];
        let command = DrawCommand::GlyphRun {
            pos: Point::ZERO,
            transform: Transform::IDENTITY,
            glyphs: &glyphs,
            font: &font,
            color: red(),
            opa: 255,
        };

        struct FontResolver;
        impl ResourceResolver for FontResolver {
            fn resolve_font(&mut self, _: &Font) -> ResourceRef {
                ResourceRef::Index(3)
            }

            fn resolve_texture(&mut self, _: &Texture<'_>) -> ResourceRef {
                unreachable!()
            }
        }
        let recorded = record_command(&command, &mut FontResolver).unwrap();
        let SceneOp::GlyphRun {
            font: ResourceRef::Index(3),
            ppem: 8,
            glyphs: recorded,
            ..
        } = recorded
        else {
            panic!("expected positioned glyph run")
        };
        assert_eq!(recorded.as_ref(), glyphs.as_slice());
    }

    #[test]
    fn posed_glyphs_retain_one_paired_geometry_buffer() {
        let font = Font::bitmap_8x8();
        let glyphs = [textflow::shaping::PositionedGlyph::new(
            GlyphId::new(65),
            textflow::shaping::FlowPoint { x: 0, y: 0 },
        )];
        let frames = [textflow::placement::GlyphFrame {
            local_origin: textflow::shaping::FlowPoint {
                x: 4 << 8,
                y: 7 << 8,
            },
            unit_tangent: textflow::shaping::FlowPoint { x: 0, y: 1 << 8 },
        }];
        let command = DrawCommand::PosedGlyphRun {
            pos: Point::ZERO,
            transform: Transform::IDENTITY,
            glyphs: crate::render::PosedGlyphs::new(&glyphs, &frames).unwrap(),
            font: &font,
            color: red(),
            opa: 255,
        };

        struct FontResolver;
        impl ResourceResolver for FontResolver {
            fn resolve_font(&mut self, _: &Font) -> ResourceRef {
                ResourceRef::Index(3)
            }

            fn resolve_texture(&mut self, _: &Texture<'_>) -> ResourceRef {
                unreachable!()
            }
        }

        let recorded = record_command(&command, &mut FontResolver).unwrap();
        let SceneOp::PosedGlyphRun {
            glyphs: recorded, ..
        } = recorded
        else {
            panic!("expected posed glyph run")
        };
        assert_eq!(recorded.glyphs(), glyphs);
        assert_eq!(recorded.frames(), frames);
    }
}
