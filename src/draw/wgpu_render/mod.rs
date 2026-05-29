//! wgpu-backed Renderer + Canvas.

mod pipeline;

use crate::app::RendererFactory;
use crate::draw::canvas::Canvas;
use crate::draw::command::DrawCommand;
use crate::draw::path::Path;
use crate::draw::renderer::Renderer;
use crate::draw::texture::Texture;
use crate::surface::wgpu_surface::WgpuSurface;
use crate::types::{Color, Fixed, Point, Rect, Viewport};

pub struct WgpuRendererFactory {
    _todo: (),
}

impl WgpuRendererFactory {
    pub fn new() -> Self {
        Self { _todo: () }
    }
}

impl Default for WgpuRendererFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl RendererFactory<WgpuSurface> for WgpuRendererFactory {
    type Renderer<'a>
        = WgpuRenderer<'a>
    where
        Self: 'a;

    fn make<'a>(
        &'a mut self,
        _backend: &'a mut WgpuSurface,
        _transform: &Viewport,
    ) -> WgpuRenderer<'a> {
        todo!()
    }
}

pub struct WgpuRenderer<'a> {
    _factory: &'a mut WgpuRendererFactory,
    _surface: &'a mut WgpuSurface,
}

impl Renderer for WgpuRenderer<'_> {
    fn draw(&mut self, _cmd: &DrawCommand, _clip: &Rect) {
        todo!()
    }

    fn flush(&mut self) {
        todo!()
    }
}

impl Canvas for WgpuRenderer<'_> {
    fn fill_path(&mut self, _path: &Path, _clip: &Rect, _color: &Color, _opa: u8) {
        todo!()
    }

    fn stroke_path(&mut self, _path: &Path, _clip: &Rect, _width: Fixed, _color: &Color, _opa: u8) {
        todo!()
    }

    fn blit(
        &mut self,
        _src: &Texture,
        _src_rect: &Rect,
        _dst: Point,
        _dst_size: Point,
        _clip: &Rect,
    ) {
        todo!()
    }

    fn clear(&mut self, _area: &Rect, _color: &Color) {
        todo!()
    }

    fn draw_label(&mut self, _pos: &Point, _text: &[u8], _clip: &Rect, _color: &Color, _opa: u8) {
        todo!()
    }

    fn flush(&mut self) {
        Renderer::flush(self)
    }
}
