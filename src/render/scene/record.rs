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
#[path = "../../../tests/unit/render/scene/record.rs"]
mod tests;
