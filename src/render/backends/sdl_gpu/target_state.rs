use core::ptr::{self, NonNull};
use sdl2::render::{Canvas, RenderTarget};
use sdl2_sys::*;

use crate::render::RenderError;

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
        let raw = canvas.raw();
        // The binding state retains every texture that can be restored here.
        unsafe {
            checked(SDL_SetRenderTarget(raw, self.target))?;
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
        checked(SDL_SetRenderTarget(raw, texture.raw()))?;
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
