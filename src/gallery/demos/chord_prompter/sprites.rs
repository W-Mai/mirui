use crate::core::{
    cache::Handle,
    resource::{LoadError, ResourceManager},
};
use crate::prelude::*;
use crate::render::{
    command::{CompositeMode, DrawCommand},
    renderer::{DrawRequest, Renderer},
    sw::SwRenderer,
    texture::{AlphaMode, ColorFormat, Texture},
};
use crate::types::Transform;
use alloc::rc::Rc;
use core::{
    cell::Cell,
    task::{Context, Poll, Waker},
};

const TOKEN: &str = "chord-prompter/arrow-heads";
const VELOCITIES: [u8; 8] = [30, 40, 50, 60, 70, 80, 90, 100];
const TILE: usize = 5;
const TILE_BYTES: usize = TILE * TILE * 4;
const TILES: usize = 4 * VELOCITIES.len() * 2;
// A vertical strip keeps each tile tightly packed for borrowed texture views.
const ATLAS_HEIGHT: u16 = (TILE * TILES) as u16;

#[crate::compose]
pub(super) fn install() {
    if !res!(optional ResourceManager<Texture<'static>>).with(|images| images.is_some()) {
        let mut fallback = Texture::from_ref(&[0; 4], 1, 1, ColorFormat::RGBA8888)
            .expect("static transparent pixel");
        fallback.alpha_mode = AlphaMode::Blend;
        res!().insert(ResourceManager::new(
            crate::core::cache::MaxSize::Bytes(8192),
            fallback,
        ));
    }
    if !res!(optional Sprites).with(|sprites| sprites.is_some()) {
        let colors = res!(crate::ui::Theme).with(|theme| {
            [
                theme.resolve(ColorToken::OnSurfaceVariant),
                theme.resolve(ColorToken::Primary),
            ]
        });
        let sprites = res!(ResourceManager<Texture<'static>>).with(|images| {
            let sprites = Sprites::new(&images, colors);
            let _ = sprites.atlas(&images, colors);
            sprites
        });
        res!().insert(sprites);
    }
}

pub(super) fn eligible(scale: Fixed, transform: Transform, quad: Option<[Point; 4]>) -> bool {
    scale == Fixed::ONE
        && quad.is_none()
        && transform.m00 == Fixed::ONE
        && transform.m11 == Fixed::ONE
        && transform.m01 == Fixed::ZERO
        && transform.m10 == Fixed::ZERO
        && transform.tx.is_integer()
        && transform.ty.is_integer()
}

pub(super) struct Sprites {
    colors: Rc<Cell<[Color; 2]>>,
    rejected: Cell<bool>,
}

impl Sprites {
    pub(super) fn new(images: &ResourceManager<Texture<'static>>, colors: [Color; 2]) -> Self {
        let colors = Rc::new(Cell::new(colors));
        let source = colors.clone();
        images.add_loader(move |token: &str| {
            if token != TOKEN {
                return Err(LoadError::NotMine);
            }
            bake(source.get()).map_err(|_| LoadError::Failed("arrow sprite rasterization failed"))
        });
        Self {
            colors,
            rejected: Cell::new(false),
        }
    }

    pub(super) fn atlas(
        &self,
        images: &ResourceManager<Texture<'static>>,
        colors: [Color; 2],
    ) -> Option<Handle<Texture<'static>>> {
        if self.colors.get() != colors {
            self.colors.set(colors);
            self.rejected.set(false);
            // Reload intentionally keeps stale pixels until polling; this palette must resolve now.
            images.remove_token(TOKEN);
        }
        if self.rejected.get() {
            return None;
        }
        let atlas = images.resolve(TOKEN);
        if atlas.is_invalid() || atlas.width() != TILE as u16 || atlas.height() != ATLAS_HEIGHT {
            self.rejected.set(true);
            return None;
        }
        Some(atlas)
    }
}

fn revision(colors: [Color; 2]) -> u64 {
    let pack = |c: Color| u64::from(u32::from_le_bytes([c.r, c.g, c.b, c.a]));
    pack(colors[0]) | (pack(colors[1]) << 32)
}

fn bake(colors: [Color; 2]) -> Result<Texture<'static>, crate::render::RenderError> {
    let mut renderer = SwRenderer::new(Texture::owned(
        TILE as u16,
        ATLAS_HEIGHT,
        ColorFormat::RGBA8888,
    )?);
    renderer.target.alpha_mode = AlphaMode::Blend;
    for (role, color) in colors.into_iter().enumerate() {
        for (velocity_index, velocity) in VELOCITIES.into_iter().enumerate() {
            for direction in 0..4 {
                let index = (role * VELOCITIES.len() + velocity_index) * 4 + direction;
                let y = (index * TILE) as i32;
                let right = direction & 1 != 0;
                let up = direction & 2 != 0;
                let x0 = if right { 1 } else { 4 };
                let x1 = if right { 4 } else { 1 };
                let y0 = if up { 1 } else { 4 };
                let y1 = if up { 4 } else { 1 };
                let command = DrawCommand::Line {
                    p1: Point::new(x0, y + y0),
                    p2: Point::new(x1, y + y1),
                    transform: Transform::IDENTITY,
                    color,
                    width: 2.into(),
                    opa: 100 + velocity,
                };
                match renderer.poll_draw_request(
                    DrawRequest::new(&command, Rect::new(0, y, TILE as i32, TILE as i32)),
                    None,
                    &mut Context::from_waker(Waker::noop()),
                ) {
                    Poll::Ready(Ok(())) => {}
                    Poll::Ready(Err(error)) => return Err(error),
                    Poll::Pending => return Err(crate::render::RenderError::ResourcePending),
                }
                let mut pixels = renderer.target.pixels_mut()?;
                for pixel in pixels.bytes_mut()[index * TILE_BYTES..(index + 1) * TILE_BYTES]
                    .chunks_exact_mut(4)
                {
                    // Retain the straight source color independently of transparent-target rounding.
                    pixel[..3].copy_from_slice(&[color.r, color.g, color.b]);
                }
            }
        }
    }
    renderer.target.cache_revision = revision(colors);
    renderer.target.transient = false;
    Ok(renderer.target)
}

#[cfg(test)]
mod tests;

pub(super) fn draw_heads(
    atlas: &Texture<'_>,
    point: Point,
    up: bool,
    velocity: u8,
    active: bool,
    transform: Transform,
    mut draw: impl FnMut(&DrawCommand<'_>),
) -> bool {
    let Some(velocity) = VELOCITIES.iter().position(|value| *value == velocity) else {
        return false;
    };
    let Ok(pixels) = atlas.read_pixels() else {
        return false;
    };
    if pixels.bytes().len() != TILES * TILE_BYTES {
        return false;
    }
    let base = (usize::from(active) * VELOCITIES.len() + velocity) * 4 + usize::from(up) * 2;
    for right in 0..2 {
        let index = base + right;
        let Ok(mut texture) = Texture::from_ref(
            &pixels.bytes()[index * TILE_BYTES..(index + 1) * TILE_BYTES],
            TILE as u16,
            TILE as u16,
            ColorFormat::RGBA8888,
        ) else {
            return false;
        };
        texture.alpha_mode = AlphaMode::Blend;
        texture.cache_revision = atlas.cache_revision;
        texture.transient = false;
        draw(&DrawCommand::Blit {
            pos: Point::new(
                point.x - Fixed::from_int(if right == 0 { 4 } else { 1 }),
                point.y - Fixed::from_int(if up { 1 } else { 4 }),
            ),
            size: Point::new(TILE as i32, TILE as i32),
            transform,
            quad: None,
            texture: &texture,
            opa: 255,
            radius: Fixed::ZERO,
            composite: CompositeMode::SourceOver,
        });
    }
    true
}
