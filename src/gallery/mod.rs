use alloc::vec::Vec;
use core::cell::RefCell;

pub mod demos;

pub(crate) struct SceneRgbaScratch(RefCell<Vec<u8>>);

impl SceneRgbaScratch {
    #[cfg(feature = "std")]
    pub(crate) fn new(bytes: usize) -> Self {
        Self(RefCell::new(alloc::vec![0; bytes]))
    }

    #[cfg(feature = "std")]
    pub(crate) fn ensure_capacity(&mut self, bytes: usize) {
        if self.0.get_mut().len() < bytes {
            self.0.get_mut().resize(bytes, 0);
        }
    }

    pub(crate) fn with_surface_rgba<R>(
        &self,
        rect: crate::types::Rect,
        scale: crate::types::Fixed,
        f: impl FnOnce(&mut [u8]) -> R,
    ) -> R {
        let width = usize::try_from(
            (rect.w.max(crate::types::Fixed::ZERO) * scale)
                .ceil()
                .to_int(),
        )
        .unwrap_or(0);
        let height = usize::try_from(
            (rect.h.max(crate::types::Fixed::ZERO) * scale)
                .ceil()
                .to_int(),
        )
        .unwrap_or(0);
        let required = width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4))
            .unwrap_or(0);
        let mut rgba = self.0.borrow_mut();
        if rgba.len() < required {
            rgba.resize(required, 0);
        }
        f(rgba.as_mut_slice())
    }

    #[cfg(feature = "std")]
    pub(crate) fn install(world: &mut crate::ecs::World, bytes: usize) {
        if let Some(scratch) = world.resource_mut::<Self>() {
            scratch.ensure_capacity(bytes);
        } else {
            world.insert_resource(Self::new(bytes));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Fixed, Rect};

    #[test]
    fn surface_scratch_grows_for_physical_pixels_without_shrinking() {
        let scratch = SceneRgbaScratch::new(16);
        let rect = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(3),
            h: Fixed::from_int(2),
        };

        scratch.with_surface_rgba(rect, Fixed::from_int(2), |rgba| {
            assert_eq!(rgba.len(), 6 * 4 * 4);
        });
        scratch.with_surface_rgba(rect, Fixed::ONE, |rgba| {
            assert_eq!(rgba.len(), 6 * 4 * 4);
        });
    }
}
