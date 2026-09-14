use super::SwRenderer;
use super::backends::sw::SwScratch;
use super::canvas::Canvas;
use super::renderer::Renderer;
use crate::surface::{FramebufferAccess, Surface};
use crate::types::Viewport;

/// Builds a Renderer each frame, given mutable access to the backend and
/// the current logical/physical coord transform.
///
/// The factory is parameterised over the backend type so each GPU backend
/// can bind to its own concrete `B` and reach into backend-specific
/// resources (SDL canvas, wgpu device, VG-Lite context). CPU-raster
/// factories (like [`SwRendererFactory`]) use the [`FramebufferAccess`]
/// sub-trait bound to obtain a `Texture<'_>` from any compatible backend.
pub trait RendererFactory<B: Surface> {
    type Renderer<'a>: Renderer + Canvas
    where
        Self: 'a,
        B: 'a;
    fn make<'a>(&'a mut self, backend: &'a mut B, transform: &Viewport) -> Self::Renderer<'a>;

    /// Mirror the frame's dirty rects into inactive slots and rotate.
    /// Default no-op — only multi-buffer CPU backends override.
    fn mirror_and_advance(
        &mut self,
        _backend: &mut B,
        _plan: &crate::ui::dirty::DirtyRegions,
        _transform: &Viewport,
    ) {
    }
}

/// Default factory that retains software path and clip scratch across frames
/// for any backend exposing a CPU framebuffer.
pub struct SwRendererFactory {
    scratch: SwScratch,
}

impl SwRendererFactory {
    pub fn new() -> Self {
        Self {
            scratch: SwScratch::new(),
        }
    }
}

impl Default for SwRendererFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: FramebufferAccess> RendererFactory<B> for SwRendererFactory {
    type Renderer<'a>
        = SwRenderer<'a>
    where
        Self: 'a,
        B: 'a;
    fn make<'a>(&'a mut self, backend: &'a mut B, transform: &Viewport) -> SwRenderer<'a> {
        let tex = backend.framebuffer();
        let mut r = SwRenderer::with_scratch(tex, &mut self.scratch);
        r.viewport = *transform;
        r
    }

    fn mirror_and_advance(
        &mut self,
        backend: &mut B,
        plan: &crate::ui::dirty::DirtyRegions,
        transform: &Viewport,
    ) {
        if backend.buffer_count() <= 1 {
            return;
        }
        {
            let mut bufs = backend.all_buffers();
            if let Some((active, inactives)) = bufs.split_first_mut() {
                // Shift before blit — reversing smears the dirty pixels.
                let scale = transform.scale();
                for shift in &plan.shifts {
                    let (x0, y0, x1, y1) = transform.rect_to_physical_pixel_bounds(shift.area);
                    let dx_phys = (shift.dx * scale).trunc_to_int();
                    let dy_phys = (shift.dy * scale).trunc_to_int();
                    for inact in inactives.iter_mut() {
                        crate::surface::mirror::texture_scroll_in_place(
                            inact, x0, y0, x1, y1, dx_phys, dy_phys,
                        );
                    }
                }
                for rect in &plan.rects {
                    let (x0, y0, x1, y1) = transform.rect_to_physical_pixel_bounds(*rect);
                    for inact in inactives.iter_mut() {
                        crate::surface::mirror::blit_region(inact, active, x0, y0, x1, y1);
                    }
                }
            }
        }
        backend.advance();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::canvas::Paint;
    use crate::render::path::Path;
    use crate::render::raster::{FillRule, LineCap, LineJoin};
    use crate::surface::framebuf::FramebufSurface;
    use crate::types::{Color, Fixed, Rect};

    #[test]
    fn software_factory_retains_path_and_clip_buffers_between_frames() {
        let mut surface = FramebufSurface::new(64, 64, |_, _| {});
        let mut factory = SwRendererFactory::new();
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        let clip = Rect::new(0, 0, 64, 64);
        let path = Path::rect(8.into(), 8.into(), 40.into(), 40.into());
        let paint = Paint::Color(Color::rgb(20, 30, 40).into());
        let dash = [Fixed::from_int(4), Fixed::from_int(2)];
        let mut first = None;

        for _ in 0..2 {
            let mut renderer = factory.make(&mut surface, &viewport);
            renderer.push_clip(&path, &crate::types::Transform::IDENTITY, FillRule::EvenOdd);
            renderer.fill_path(&path, &clip, &paint, 255, FillRule::EvenOdd);
            renderer.stroke_path(
                &path,
                &clip,
                Fixed::from_int(3),
                &paint,
                255,
                LineCap::Round,
                LineJoin::Round,
                Fixed::from_int(4),
                &dash,
            );
            renderer.pop_clip();
            drop(renderer);

            let state = factory.scratch.retained_buffer_state();
            if let Some(first) = first {
                assert_eq!(state, first);
            } else {
                first = Some(state);
            }
        }
    }
}
