use super::SpriteStack;
use crate::render::RenderError;
use crate::render::command::{CompositeMode, DrawCommand};
use crate::render::sprite_stack::grid::GridBasis;
use crate::render::texture::{AlphaMode, Texture, TexturePixels};
use crate::types::{Fixed, Point, Rect, Transform, fixed::storage::to_i32};
use crate::ui::view::PaintCtx;

fn sin_cos(degrees: Fixed) -> (Fixed, Fixed) {
    const QUARTER: i32 = to_i32(Fixed::from_int(90));
    const HALF: i32 = 2 * QUARTER;
    const THREE_QUARTERS: i32 = 3 * QUARTER;
    let raw = to_i32(degrees).rem_euclid(4 * QUARTER);
    match raw {
        0 => (Fixed::ZERO, Fixed::ONE),
        QUARTER => (Fixed::ONE, Fixed::ZERO),
        HALF => (Fixed::ZERO, -Fixed::ONE),
        THREE_QUARTERS => (-Fixed::ONE, Fixed::ZERO),
        _ => {
            let angle = Fixed::from_ratio(raw, 256);
            (Fixed::sin_deg(angle), Fixed::cos_deg(angle))
        }
    }
}

/// Checks fixed-point projection range before camera basis arithmetic.
fn fits(canvas: (u16, u16), pivot: [i32; 2], height: i64, scale: Fixed, center: Point) -> bool {
    let extent =
        [canvas.0, canvas.1]
            .into_iter()
            .zip(pivot)
            .fold(height, |sum, (length, pivot)| {
                let pivot = i64::from(pivot);
                sum.saturating_add(pivot.abs().max((i64::from(length) * 256 - pivot).abs()))
            });
    let screen = extent.saturating_mul(i64::from(to_i32(scale))) / 256;
    let center = i64::from(to_i32(center.x))
        .abs()
        .max(i64::from(to_i32(center.y)).abs());
    screen.saturating_add(center) < i64::from(i32::MAX) - 256
}

pub(super) fn paint(sprite: &SpriteStack, paint: &mut PaintCtx<'_>) -> Result<(), RenderError> {
    let Some(asset) = sprite.asset.as_ref() else {
        return Ok(());
    };
    let rect = paint.rect();
    let center = Point::new(rect.x + rect.w / 2, rect.y + rect.h / 2);
    if !rect.w.is_positive() || !rect.h.is_positive() || sprite.scale.is_zero() {
        return Ok(());
    }
    let config = asset.descriptor();
    let spacing = sprite
        .layer_spacing
        .0
        .unwrap_or_else(|| Fixed::from_ratio(config.spacing_q8(), 256));
    if spacing.is_negative() || sprite.scale.is_negative() {
        return Err(RenderError::InvalidGeometry);
    }
    let count = config.layers().len();
    let height = i64::from(to_i32(spacing)) * (count as i64 - 1);
    if height > i64::from(i32::MAX)
        || !fits(
            config.canvas(),
            config.pivot_q8(),
            height,
            sprite.scale,
            center,
        )
    {
        return Err(RenderError::InvalidGeometry);
    }
    let (sy, cy) = sin_cos(sprite.yaw);
    let (sp, cp) = sin_cos(
        sprite
            .pitch
            .clamp(Fixed::from_int(-90), Fixed::from_int(90)),
    );
    let px = Fixed::from_ratio(config.pivot_q8()[0], 256);
    let py = Fixed::from_ratio(config.pivot_q8()[1], 256);
    let scale = sprite.scale;
    let col = [cy * scale, sy * sp * scale, sy];
    let row = [-sy * scale, cy * sp * scale, cy];
    let basis = GridBasis::new(col, row);
    let x0 = center.x - col[0] * px - row[0] * py;
    let y0 = center.y - col[1] * px - row[1] * py;
    let transform = paint.transform();
    for step in 0..count {
        let index = if sp.is_negative() {
            count - 1 - step
        } else {
            step
        };
        let region = config.layers().get(index).expect("validated sprite layer");
        if region.width() == 0 {
            continue;
        }
        let rows = asset
            .atlas()
            .pixels_in(region)?
            .ok_or(RenderError::ResourcePending)?;
        let pixels = rows.pixels().ok_or(RenderError::InvalidTexture)?;
        let bottom = y0 - spacing * index as i32 * cp * scale;
        if sp.is_zero() {
            edge_on(
                paint,
                &pixels,
                &basis,
                Point::new(x0, bottom),
                [sy, cy],
                scale,
                transform,
            );
        } else {
            let mut texture = Texture::from_ref(
                pixels.bytes(),
                pixels.width(),
                pixels.height(),
                pixels.format(),
            )?
            .with_stride(pixels.stride())?
            .with_cache_revision(asset.layer_revision(index));
            texture.alpha_mode = AlphaMode::Blend;
            let layer_transform = Transform {
                m00: col[0],
                m01: row[0],
                m10: col[1],
                m11: row[1],
                tx: x0,
                ty: bottom,
            };
            paint.draw(&DrawCommand::Blit {
                pos: Point::ZERO,
                size: Point::new(Fixed::from(pixels.width()), Fixed::from(pixels.height())),
                transform: transform.compose(&layer_transform),
                quad: None,
                texture: &texture,
                radius: Fixed::ZERO,
                opa: 255,
                composite: CompositeMode::SourceOver,
            });
        }
    }
    Ok(())
}

/// At zero elevation a slice is edge-on. Paint the projected occupied cells
/// from the back of the slice forward, with one model pixel of vertical extent.
fn edge_on(
    paint: &mut PaintCtx<'_>,
    pixels: &TexturePixels<'_>,
    basis: &GridBasis,
    origin: Point,
    depth: [Fixed; 2],
    scale: Fixed,
    transform: Transform,
) {
    let [offset, right] = basis.axis_range(Fixed::ZERO, 0, [0, 1], [0, 1]);
    let width = right - offset;
    let w = i32::from(pixels.width());
    let h = i32::from(pixels.height());
    for row in 0..h {
        let y = if depth[1].is_negative() {
            h - 1 - row
        } else {
            row
        };
        for column in 0..w {
            let x = if depth[0].is_negative() {
                w - 1 - column
            } else {
                column
            };
            let color = pixels.get_pixel(x, y);
            if color.a == 0 {
                continue;
            }
            let left = basis.axis(origin.x, 0, x, y) + offset;
            paint.draw(&DrawCommand::Fill {
                area: Rect::new(left, origin.y - scale / 2, width, scale),
                transform,
                quad: None,
                color,
                radius: Fixed::ZERO,
                opa: 255,
            });
        }
    }
}
