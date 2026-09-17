use alloc::vec::Vec;
use core::cell::RefCell;

pub mod demos;

pub(crate) fn fit_logical_canvas(
    rect: crate::types::Rect,
    parent: crate::types::Transform,
    width: i32,
    height: i32,
) -> crate::types::Transform {
    let logical_width = crate::types::Fixed::from_int(width);
    let logical_height = crate::types::Fixed::from_int(height);
    let scale = (rect.w / logical_width).min(rect.h / logical_height);
    let x = rect.x + (rect.w - logical_width * scale) / crate::types::Fixed::from_int(2);
    let y = rect.y + (rect.h - logical_height * scale) / crate::types::Fixed::from_int(2);
    parent
        .compose(&crate::types::Transform::translate(x, y))
        .compose(&crate::types::Transform::scale(scale, scale))
}

pub(crate) struct SceneReplayWorkspace(RefCell<Vec<u8>>);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneWorkspaceError {
    SizeOverflow,
    InsufficientCapacity { required: usize, prepared: usize },
}

impl SceneReplayWorkspace {
    #[cfg(feature = "std")]
    pub(crate) fn new(bytes: usize) -> Self {
        Self(RefCell::new(alloc::vec![0; bytes]))
    }

    #[cfg(feature = "std")]
    pub(crate) fn prepare(&mut self, bytes: usize) {
        if self.0.get_mut().len() < bytes {
            self.0.get_mut().resize(bytes, 0);
        }
    }

    pub(crate) fn with_prepared_surface<R>(
        &self,
        rect: crate::types::Rect,
        scale: crate::types::Fixed,
        f: impl FnOnce(&mut [u8]) -> R,
    ) -> Result<R, SceneWorkspaceError> {
        let required = Self::required_bytes(rect, scale)?;
        let mut rgba = self.0.borrow_mut();
        if rgba.len() < required {
            return Err(SceneWorkspaceError::InsufficientCapacity {
                required,
                prepared: rgba.len(),
            });
        }
        Ok(f(&mut rgba[..required]))
    }

    fn required_bytes(
        rect: crate::types::Rect,
        scale: crate::types::Fixed,
    ) -> Result<usize, SceneWorkspaceError> {
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
        width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(SceneWorkspaceError::SizeOverflow)
    }

    #[cfg(feature = "std")]
    pub(crate) fn install(
        world: &mut crate::ecs::World,
        rect: crate::types::Rect,
        scale: crate::types::Fixed,
    ) -> Result<(), SceneWorkspaceError> {
        let bytes = Self::required_bytes(rect, scale)?;
        if let Some(scratch) = world.resource_mut::<Self>() {
            scratch.prepare(bytes);
        } else {
            world.insert_resource(Self::new(bytes));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Fixed, Rect};

    #[test]
    fn logical_canvas_is_centered_and_contained() {
        let rect = Rect::new(10, 20, 300, 500);
        let transform = fit_logical_canvas(rect, crate::types::Transform::IDENTITY, 400, 200);
        let top_left = transform.apply_point(crate::types::Point::ZERO);
        let bottom_right = transform.apply_point(crate::types::Point::new(400, 200));
        assert_eq!(top_left.x, rect.x);
        assert_eq!(top_left.y, Fixed::from_int(195));
        assert_eq!(bottom_right.x, rect.x + rect.w);
        assert_eq!(bottom_right.y, Fixed::from_int(345));
    }

    #[test]
    fn prepared_surface_never_grows_during_render() {
        let mut scratch = SceneReplayWorkspace::new(16);
        let rect = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(3),
            h: Fixed::from_int(2),
        };

        let required = SceneReplayWorkspace::required_bytes(rect, Fixed::from_int(2)).unwrap();
        scratch.prepare(required);
        scratch
            .with_prepared_surface(rect, Fixed::from_int(2), |rgba| {
                assert_eq!(rgba.len(), 6 * 4 * 4);
            })
            .unwrap();
        assert_eq!(scratch.0.get_mut().len(), required);

        let larger = Rect {
            w: Fixed::from_int(4),
            ..rect
        };
        assert_eq!(
            scratch.with_prepared_surface(larger, Fixed::from_int(2), |_| ()),
            Err(SceneWorkspaceError::InsufficientCapacity {
                required: 8 * 4 * 4,
                prepared: required,
            })
        );
        assert_eq!(scratch.0.get_mut().len(), required);
    }
}
