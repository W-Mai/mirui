//! wgpu-backed Surface.

use super::{BackbufferPersistence, DisplayInfo, InputEvent, Surface};
use crate::cache::InspectCaches;
use crate::types::Rect;

pub struct WgpuSurface {
    _todo: (),
}

impl InspectCaches for WgpuSurface {}

impl Surface for WgpuSurface {
    fn display_info(&self) -> DisplayInfo {
        todo!()
    }

    fn flush(&mut self, _area: &Rect) {
        todo!()
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        todo!()
    }

    fn persistence(&self) -> BackbufferPersistence {
        BackbufferPersistence::Transient
    }
}
