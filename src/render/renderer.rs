use crate::types::{Fixed, Rect};

use super::command::DrawCommand;
use super::texture::ColorFormat;

pub trait Renderer {
    fn draw(&mut self, cmd: &DrawCommand, clip: &Rect);
    fn flush(&mut self);

    /// Whether this backend serves
    /// [`crate::ui::OffscreenRender`] entities through the SW
    /// pipeline: an inner `SwRenderer` over an owned buffer, blit'd
    /// back via [`Self::draw`] with `DrawCommand::Blit`. Returning
    /// `false` makes the render walker skip the offscreen path
    /// entirely and inline-render the subtree.
    fn supports_offscreen(&self) -> bool {
        false
    }

    /// Format the offscreen buffer should be allocated in. `None`
    /// means the backend does not host offscreen rendering and the
    /// walker should not call [`Self::supports_offscreen`].
    fn offscreen_format(&self) -> Option<ColorFormat> {
        None
    }

    /// Copy a logical-pixel rect from the current target into `dst`.
    /// `dst` is sized in physical pixels by the caller — usually
    /// because they already own the buffer (offscreen pre-seed). A
    /// backend that returns `Some` from [`Self::offscreen_format`]
    /// must override this. Effects that just want to grab a region
    /// of the framebuffer should use [`Self::sample_target_region`]
    /// instead.
    fn read_target_region(&self, _src: &Rect, _dst: &mut crate::render::texture::Texture) {
        unimplemented!("Renderer::read_target_region not implemented for this backend")
    }

    /// Logical-pixel `src` in, physical-resolution texture out.
    /// Backends that can't read their own target should return `None`;
    /// the default panics so a missing override is loud, not silent.
    fn sample_target_region(
        &self,
        _src: &Rect,
    ) -> Option<crate::render::texture::Texture<'static>> {
        unimplemented!("Renderer::sample_target_region not implemented for this backend")
    }

    /// Hand the closure a mutable `Texture` view over physical-pixel
    /// framebuffer bytes inside `src`, skipping the alloc-and-blit-back
    /// round-trip that `sample_target_region` + `draw(Blit)` would do.
    /// Returns `true` when the closure ran. Default panics so a missing
    /// override is loud.
    fn modify_target_region(
        &mut self,
        _src: &Rect,
        _f: &mut dyn FnMut(&mut crate::render::texture::Texture),
    ) -> bool {
        unimplemented!("Renderer::modify_target_region not implemented for this backend")
    }

    /// Backends that defer draws (currently only `wgpu`) need to submit
    /// pending work before a `sample_target_region` / `read_target_region`
    /// call can see this frame's pixels. Eager backends keep the default
    /// no-op. Callers running readback should invoke this immediately
    /// before the read; idempotent if no work is pending.
    fn prepare_readback(&mut self, _src: &Rect) {}

    /// Whether this backend implements `scroll_target_region`. The
    /// dirty walker checks this before emitting a `RegionShift`.
    fn supports_scroll_blit(&self) -> bool {
        false
    }

    /// Shift `area`'s pixels by `(dx, dy)` *logical* pixels (memmove,
    /// no draw, no flush). Pixels evicted from `area` are dropped; the
    /// caller repaints the newly exposed strip(s). Default panics;
    /// backends opt in by overriding both this and
    /// `supports_scroll_blit()`.
    fn scroll_target_region(&mut self, _area: &Rect, _dx: Fixed, _dy: Fixed) {
        unimplemented!("Renderer::scroll_target_region not implemented for this backend")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopRenderer;
    impl Renderer for NoopRenderer {
        fn draw(&mut self, _cmd: &DrawCommand, _clip: &Rect) {}
        fn flush(&mut self) {}
    }

    #[test]
    fn default_supports_offscreen_is_false() {
        let r = NoopRenderer;
        assert!(!r.supports_offscreen());
    }
}
