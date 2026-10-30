use core::ptr::{self, NonNull};
use sdl2::render::{Canvas, RenderTarget};
use sdl2_sys::*;

use crate::render::{RenderError, TargetAlpha};
use crate::types::{Color, PhysicalRect};

pub(super) fn checked(status: i32) -> Result<(), RenderError> {
    if status == 0 {
        Ok(())
    } else {
        Err(RenderError::BackendFailure)
    }
}

pub(super) struct DriverState {
    target: *mut SDL_Texture,
    viewport: SDL_Rect,
    clip: SDL_Rect,
    clipped: bool,
    scale: (f32, f32),
    logical: (i32, i32),
    integer: SDL_bool,
    color: [u8; 4],
    blend: SDL_BlendMode,
}

impl DriverState {
    pub(super) fn capture<T: RenderTarget>(canvas: &Canvas<T>) -> Result<Self, RenderError> {
        crate::trace_span!("sdl.state.capture");
        let raw = canvas.raw();
        let mut state = Self {
            target: ptr::null_mut(),
            viewport: SDL_Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            clip: SDL_Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            clipped: false,
            scale: (0.0, 0.0),
            logical: (0, 0),
            integer: SDL_bool::SDL_FALSE,
            color: [0; 4],
            blend: SDL_BlendMode::SDL_BLENDMODE_NONE,
        };
        // Canvas owns the renderer; all output pointers refer to initialized local fields.
        unsafe {
            state.target = SDL_GetRenderTarget(raw);
            SDL_RenderGetViewport(raw, &mut state.viewport);
            SDL_RenderGetClipRect(raw, &mut state.clip);
            state.clipped = SDL_RenderIsClipEnabled(raw) == SDL_bool::SDL_TRUE;
            SDL_RenderGetScale(raw, &mut state.scale.0, &mut state.scale.1);
            SDL_RenderGetLogicalSize(raw, &mut state.logical.0, &mut state.logical.1);
            state.integer = SDL_RenderGetIntegerScale(raw);
            let [r, g, b, a] = &mut state.color;
            checked(SDL_GetRenderDrawColor(raw, r, g, b, a))?;
            checked(SDL_GetRenderDrawBlendMode(raw, &mut state.blend))?;
        }
        Ok(state)
    }

    pub(super) fn restore<T: RenderTarget>(&self, canvas: &Canvas<T>) -> Result<(), RenderError> {
        self.restore_target(canvas)?;
        self.restore_properties(canvas)
    }

    pub(super) fn restore_target<T: RenderTarget>(
        &self,
        canvas: &Canvas<T>,
    ) -> Result<(), RenderError> {
        let raw = canvas.raw();
        // The binding state retains every texture that can be restored here.
        unsafe {
            // Changing targets can submit queued drawing before restoring properties.
            crate::trace_span!("sdl.target.restore", {
                checked(SDL_SetRenderTarget(raw, self.target))
            })
        }
    }

    pub(super) fn restore_properties<T: RenderTarget>(
        &self,
        canvas: &Canvas<T>,
    ) -> Result<(), RenderError> {
        let raw = canvas.raw();
        // Restore properties without selecting another target.
        unsafe {
            crate::trace_span!("sdl.state.restore");
            checked(SDL_RenderSetIntegerScale(raw, self.integer))?;
            checked(SDL_RenderSetLogicalSize(
                raw,
                self.logical.0,
                self.logical.1,
            ))?;
            checked(SDL_RenderSetScale(raw, self.scale.0, self.scale.1))?;
            checked(SDL_RenderSetViewport(raw, &self.viewport))?;
            checked(SDL_RenderSetClipRect(
                raw,
                if self.clipped {
                    &self.clip
                } else {
                    ptr::null()
                },
            ))?;
            checked(SDL_SetRenderDrawColor(
                raw,
                self.color[0],
                self.color[1],
                self.color[2],
                self.color[3],
            ))?;
            checked(SDL_SetRenderDrawBlendMode(raw, self.blend))?;
        }
        Ok(())
    }
}

pub(super) fn select<T: RenderTarget>(
    canvas: &Canvas<T>,
    texture: &sdl2::render::Texture<'_>,
) -> Result<NonNull<SDL_Texture>, RenderError> {
    let raw = canvas.raw();
    // Both handles belong to the checked source device and remain alive for this operation.
    unsafe {
        crate::trace_span!("sdl.target.select", {
            checked(SDL_SetRenderTarget(raw, texture.raw()))
        })?;
        crate::trace_span!("sdl.state.select");
        checked(SDL_RenderSetIntegerScale(raw, SDL_bool::SDL_FALSE))?;
        checked(SDL_RenderSetLogicalSize(raw, 0, 0))?;
        checked(SDL_RenderSetScale(raw, 1.0, 1.0))?;
        checked(SDL_RenderSetViewport(raw, ptr::null()))?;
        checked(SDL_RenderSetClipRect(raw, ptr::null()))?;
        // SDL may bind a native-format texture behind the requested texture.
        NonNull::new(SDL_GetRenderTarget(raw)).ok_or(RenderError::InvalidScope)
    }
}

pub(super) fn is_current<T: RenderTarget>(
    canvas: &Canvas<T>,
    target: NonNull<SDL_Texture>,
) -> bool {
    // Comparing live handles does not dereference a previously saved target.
    unsafe { SDL_GetRenderTarget(canvas.raw()) == target.as_ptr() }
}

pub(super) fn clear<T: RenderTarget>(canvas: &Canvas<T>) -> Result<(), RenderError> {
    // SDL_RenderClear replaces the complete target, independently of clipping and blending.
    unsafe {
        checked(SDL_SetRenderDrawColor(canvas.raw(), 0, 0, 0, 0))?;
        checked(SDL_RenderClear(canvas.raw()))
    }
}

pub(super) fn clear_region<T: RenderTarget>(
    canvas: &Canvas<T>,
    area: PhysicalRect,
    color: Color,
    alpha: TargetAlpha,
) -> Result<(), RenderError> {
    let saved = DriverState::capture(canvas)?;
    let [r, g, b, a] =
        TargetAlpha::Straight.convert_rgba8(alpha, [color.r, color.g, color.b, color.a]);
    let rect = SDL_Rect {
        x: i32::from(area.x()),
        y: i32::from(area.y()),
        w: i32::from(area.width()),
        h: i32::from(area.height()),
    };
    // The canvas owns the renderer and all pointers refer to live local values.
    let result = unsafe {
        (|| {
            checked(SDL_RenderSetLogicalSize(canvas.raw(), 0, 0))?;
            checked(SDL_RenderSetScale(canvas.raw(), 1.0, 1.0))?;
            checked(SDL_RenderSetViewport(canvas.raw(), ptr::null()))?;
            checked(SDL_RenderSetClipRect(canvas.raw(), ptr::null()))?;
            checked(SDL_SetRenderDrawBlendMode(
                canvas.raw(),
                SDL_BlendMode::SDL_BLENDMODE_NONE,
            ))?;
            checked(SDL_SetRenderDrawColor(canvas.raw(), r, g, b, a))?;
            checked(SDL_RenderFillRect(canvas.raw(), &rect))
        })()
    };
    let restored = saved.restore(canvas);
    result.and(restored)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_region_overwrites_pixels_and_restores_driver_state() {
        check_clear(TargetAlpha::Straight, [200, 100, 50, 128]);
        check_clear(TargetAlpha::Premultiplied, [100, 50, 25, 128]);
    }

    fn check_clear(alpha: TargetAlpha, encoded: [u8; 4]) {
        let mut canvas = sdl2::surface::Surface::new(4, 3, sdl2::pixels::PixelFormatEnum::RGBA32)
            .unwrap()
            .into_canvas()
            .unwrap();
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(13, 29, 47, 255));
        canvas.clear();
        canvas.set_viewport(sdl2::rect::Rect::new(1, 1, 2, 2));
        canvas.set_clip_rect(sdl2::rect::Rect::new(0, 0, 1, 1));
        canvas.set_scale(1.5, 1.5).unwrap();
        canvas.set_blend_mode(sdl2::render::BlendMode::Add);
        let before = (
            canvas.viewport(),
            canvas.clip_rect(),
            canvas.scale(),
            canvas.draw_color(),
            canvas.blend_mode(),
        );
        for _ in 0..4 {
            clear_region(
                &canvas,
                PhysicalRect::new(1, 1, 3, 2).unwrap(),
                Color::rgba(200, 100, 50, 128),
                alpha,
            )
            .unwrap();
            clear_region(
                &canvas,
                PhysicalRect::new(2, 1, 1, 1).unwrap(),
                Color::rgba(0, 0, 0, 0),
                alpha,
            )
            .unwrap();
        }
        assert_eq!(
            (
                canvas.viewport(),
                canvas.clip_rect(),
                canvas.scale(),
                canvas.draw_color(),
                canvas.blend_mode()
            ),
            before
        );
        canvas.set_scale(1.0, 1.0).unwrap();
        canvas.set_viewport(Option::<sdl2::rect::Rect>::None);
        canvas.set_clip_rect(Option::<sdl2::rect::Rect>::None);
        let mut bytes = [0_u8; 4 * 3 * 4];
        super::super::readback::rgba8(&canvas, PhysicalRect::from_size(4, 3), &mut bytes, 16)
            .unwrap();
        for y in 0..3 {
            for x in 0..4 {
                let expected = if x == 2 && y == 1 {
                    [0, 0, 0, 0]
                } else if x >= 1 && y >= 1 {
                    encoded
                } else {
                    [13, 29, 47, 255]
                };
                let offset = (y * 4 + x) * 4;
                assert_eq!(&bytes[offset..offset + 4], &expected);
            }
        }
    }
}
