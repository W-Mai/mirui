use alloc::vec::Vec;
use core::cell::RefCell;

use crate::render::budget::{Allocation, MemoryBudget};
use crate::render::texture::{AlphaMode, Texture};
use crate::render::{PlaneLayout, RenderError};

#[derive(Default)]
pub(crate) struct PaintWorkspace {
    bytes: RefCell<Vec<u8>>,
    allocation: Option<Allocation>,
}

impl PaintWorkspace {
    pub(crate) fn with_bytes<T>(
        &self,
        required: usize,
        body: impl FnOnce(&mut [u8]) -> Result<T, RenderError>,
    ) -> Result<T, RenderError> {
        let mut bytes = self
            .bytes
            .try_borrow_mut()
            .map_err(|_| RenderError::InvalidScope)?;
        let bytes = bytes
            .get_mut(..required)
            .ok_or(RenderError::MissingWorkspace)?;
        body(bytes)
    }

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

    pub(crate) fn capacity(&self) -> usize {
        self.allocation.as_ref().map_or(0, Allocation::bytes)
    }

    pub(crate) fn release(&mut self) -> usize {
        let bytes = self.bytes.get_mut();
        let released = bytes.capacity();
        *bytes = Vec::new();
        self.allocation = None;
        released
    }

    pub(crate) fn allocate(&mut self, required_bytes: usize, budget: &MemoryBudget) -> bool {
        let Some(reservation) = budget.reserve(required_bytes) else {
            return false;
        };
        let bytes = self.bytes.get_mut();
        debug_assert_eq!(bytes.capacity(), 0);
        if bytes.try_reserve_exact(required_bytes).is_err() {
            return false;
        }
        let Ok(allocation) = reservation.commit(bytes.capacity()) else {
            *bytes = Vec::new();
            return false;
        };
        bytes.resize(bytes.capacity(), 0);
        self.allocation = Some(allocation);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::PlaneRequirements;
    use crate::render::texture::ColorFormat;
    use crate::test_support::tracked_allocations;
    use crate::types::Color;

    #[test]
    fn borrowed_planes_preserve_stride_and_reject_missing_capacity_and_reentry() {
        let mut workspace = PaintWorkspace::default();
        let layout =
            PlaneLayout::new(2, 2, ColorFormat::RGBA8888, 16, PlaneRequirements::CPU).unwrap();
        assert!(!workspace.with_texture(layout, |_| panic!("unreserved plane")));
        let budget = MemoryBudget::new(layout.required_bytes());
        assert!(workspace.allocate(layout.required_bytes(), &budget));
        let expected = Color::rgba(11, 29, 47, 61);
        assert!(workspace.with_texture(layout, |texture| {
            assert_eq!(texture.stride, 16);
            assert_eq!(texture.alpha_mode, AlphaMode::Blend);
            assert!(texture.transient);
            assert_eq!(workspace.capacity(), layout.required_bytes());
            texture.pixels_mut().unwrap().set_pixel(1, 1, &expected);
            assert!(!workspace.with_texture(layout, |_| panic!("nested plane lease")));
        }));
        let allocations = tracked_allocations(|| {
            assert!(workspace.with_texture(layout, |texture| {
                assert_eq!(texture.read_pixels().unwrap().get_pixel(1, 1), expected);
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
        let budget = MemoryBudget::new(layout.required_bytes());
        assert!(workspace.allocate(layout.required_bytes(), &budget));
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            workspace.with_texture(layout, |_| panic!("paint callback failed"));
        }));
        assert!(failure.is_err());
        assert!(workspace.with_texture(layout, |_| {}));
    }

    #[test]
    fn target_bytes_and_paint_planes_share_budget_without_sharing_borrows() {
        let mut target = PaintWorkspace::default();
        let mut paint = PaintWorkspace::default();
        let layout = PlaneLayout::packed(4, 4, ColorFormat::RGBA8888).unwrap();
        let budget = MemoryBudget::new(96);
        assert!(target.allocate(32, &budget));
        assert!(!paint.allocate(65, &budget));
        assert!(paint.allocate(64, &budget));
        assert_eq!(budget.available(), 0);
        assert_eq!(
            target.with_bytes(33, |_| panic!("insufficient workspace")),
            Err::<(), _>(RenderError::MissingWorkspace)
        );
        assert_eq!(
            tracked_allocations(|| {
                for _ in 0..8 {
                    target
                        .with_bytes(32, |bytes| {
                            bytes.fill(0x5a);
                            assert!(paint.with_texture(layout, |texture| {
                                texture.pixels_mut().unwrap().bytes_mut().fill(0xa5);
                            }));
                            assert!(bytes.iter().all(|byte| *byte == 0x5a));
                            assert_eq!(
                                target.with_bytes(1, |_| Ok(())),
                                Err(RenderError::InvalidScope)
                            );
                            Ok(())
                        })
                        .unwrap();
                }
            }),
            0
        );
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = target.with_bytes(32, |_| -> Result<(), RenderError> {
                panic!("binding failed")
            });
        }));
        assert!(failure.is_err());
        target.with_bytes(32, |_| Ok(())).unwrap();
        target.release();
        paint.release();
        assert_eq!(budget.available(), 96);
        assert_eq!(target.with_bytes(0, |_| Ok(())), Ok(()));
        assert_eq!(budget.allocated(), 0);
    }
}
