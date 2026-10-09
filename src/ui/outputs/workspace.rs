use alloc::vec::Vec;
use core::cell::RefCell;

use crate::render::PlaneLayout;
use crate::render::texture::{AlphaMode, Texture};

#[derive(Default)]
pub(crate) struct PaintWorkspace {
    bytes: RefCell<Vec<u8>>,
    capacity: usize,
}

impl PaintWorkspace {
    pub(crate) fn with_texture(
        &self,
        layout: PlaneLayout,
        body: impl FnOnce(&mut Texture<'_>),
    ) -> bool {
        let Ok(mut bytes) = self.bytes.try_borrow_mut() else {
            return false;
        };
        let Ok(mut plane) = layout.bind(bytes.as_mut_slice()) else {
            return false;
        };
        let mut texture = plane.texture();
        texture.alpha_mode = AlphaMode::Blend;
        body(&mut texture);
        true
    }

    pub(crate) const fn capacity(&self) -> usize {
        self.capacity
    }

    pub(crate) fn release(&mut self) -> usize {
        let bytes = self.bytes.get_mut();
        let released = bytes.capacity();
        *bytes = Vec::new();
        self.capacity = 0;
        released
    }

    pub(crate) fn allocate(&mut self, required_bytes: usize) -> bool {
        let bytes = self.bytes.get_mut();
        debug_assert_eq!(bytes.capacity(), 0);
        if bytes.try_reserve_exact(required_bytes).is_err() {
            return false;
        }
        self.capacity = bytes.capacity();
        bytes.resize(self.capacity, 0);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::PlaneRequirements;
    use crate::render::texture::{ColorFormat, TexBuf};
    use crate::test_support::tracked_allocations;
    use crate::types::Color;

    #[test]
    fn borrowed_planes_preserve_stride_and_reject_missing_capacity_and_reentry() {
        let mut workspace = PaintWorkspace::default();
        let layout =
            PlaneLayout::new(2, 2, ColorFormat::RGBA8888, 16, PlaneRequirements::CPU).unwrap();
        assert!(!workspace.with_texture(layout, |_| panic!("unreserved plane")));
        assert!(workspace.allocate(layout.required_bytes()));
        let expected = Color::rgba(11, 29, 47, 61);
        assert!(workspace.with_texture(layout, |texture| {
            assert!(matches!(texture.buf, TexBuf::Mut(_)));
            assert_eq!(texture.stride, 16);
            assert_eq!(texture.alpha_mode, AlphaMode::Blend);
            assert!(texture.transient);
            assert_eq!(workspace.capacity(), layout.required_bytes());
            texture.set_pixel(1, 1, &expected);
            assert!(!workspace.with_texture(layout, |_| panic!("nested plane lease")));
        }));
        let allocations = tracked_allocations(|| {
            assert!(workspace.with_texture(layout, |texture| {
                assert_eq!(texture.get_pixel(1, 1), expected);
            }));
        });
        assert_eq!(allocations, 0);
        assert_eq!(workspace.release(), layout.required_bytes());
        assert!(!workspace.with_texture(layout, |_| panic!("released plane")));
    }

    #[test]
    fn unwinding_a_plane_callback_restores_the_borrow() {
        let mut workspace = PaintWorkspace::default();
        let layout = PlaneLayout::packed(1, 1, ColorFormat::RGBA8888).unwrap();
        assert!(workspace.allocate(layout.required_bytes()));
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            workspace.with_texture(layout, |_| panic!("paint callback failed"));
        }));
        assert!(failure.is_err());
        assert!(workspace.with_texture(layout, |_| {}));
    }
}
