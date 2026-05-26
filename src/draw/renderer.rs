use crate::types::Rect;

use super::command::DrawCommand;
use super::texture::ColorFormat;

pub trait Renderer {
    fn draw(&mut self, cmd: &DrawCommand, clip: &Rect);
    fn flush(&mut self);

    /// Whether this backend serves
    /// [`crate::widget::OffscreenRender`] entities through the SW
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
    fn read_target_region(&self, _src: &Rect, _dst: &mut crate::draw::texture::Texture) {
        unimplemented!("Renderer::read_target_region not implemented for this backend")
    }

    /// Logical-pixel `src` in, physical-resolution texture out.
    /// Backends that can't read their own target should return `None`;
    /// the default panics so a missing override is loud, not silent.
    fn sample_target_region(&self, _src: &Rect) -> Option<crate::draw::texture::Texture<'static>> {
        unimplemented!("Renderer::sample_target_region not implemented for this backend")
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
