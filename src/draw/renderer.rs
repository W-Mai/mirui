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
