use super::SwRenderer;
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

/// Default factory that produces plain `SwRenderer<'a>` on top of any
/// backend exposing a CPU framebuffer.
pub struct SwRendererFactory;

impl<B: FramebufferAccess> RendererFactory<B> for SwRendererFactory {
    type Renderer<'a>
        = SwRenderer<'a>
    where
        Self: 'a,
        B: 'a;
    fn make<'a>(&'a mut self, backend: &'a mut B, transform: &Viewport) -> SwRenderer<'a> {
        let tex = backend.framebuffer();
        let mut r = SwRenderer::new(tex);
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
