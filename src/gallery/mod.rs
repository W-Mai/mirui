use alloc::vec::Vec;
use core::cell::RefCell;

pub mod demos;
pub(crate) mod play;
pub mod showcase_theme;

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

/// Prepares the retained scene replay buffer before rendering starts.
///
/// **Inserts**
/// - resource: [`SceneReplayWorkspace`]
/// - hooks:    `pre_render`
#[cfg(feature = "std")]
pub(crate) struct SceneReplayWorkspacePlugin;

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
    fn prepare_viewport(
        &mut self,
        viewport: crate::types::Viewport,
    ) -> Result<(), SceneWorkspaceError> {
        let (width, height) = viewport.physical_size();
        let bytes = usize::from(width)
            .checked_mul(usize::from(height))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(SceneWorkspaceError::SizeOverflow)?;
        self.prepare(bytes);
        Ok(())
    }
}

#[cfg(feature = "std")]
impl<B, F> crate::app::plugin::Plugin<B, F> for SceneReplayWorkspacePlugin
where
    B: crate::surface::Surface,
    F: crate::app::RendererFactory<B>,
{
    fn build(&mut self, app: &mut crate::app::App<B, F>) {
        let mut workspace = SceneReplayWorkspace::new(0);
        workspace
            .prepare_viewport(app.viewport())
            .expect("scene workspace viewport size is representable");
        app.world.insert_resource(workspace);
    }

    fn pre_render(&mut self, world: &mut crate::ecs::World) {
        let viewport = world
            .resource::<crate::app::RenderViewport>()
            .expect("App always owns a RenderViewport")
            .0;
        world
            .resource_mut::<SceneReplayWorkspace>()
            .expect("SceneReplayWorkspacePlugin owns its workspace")
            .prepare_viewport(viewport)
            .expect("scene workspace viewport size is representable");
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

    #[test]
    fn viewport_preparation_tracks_physical_resize() {
        let mut scratch = SceneReplayWorkspace::new(0);
        scratch
            .prepare_viewport(crate::types::Viewport::new(320, 180, Fixed::from_int(2)))
            .unwrap();
        assert_eq!(scratch.0.get_mut().len(), 320 * 180 * 4);

        scratch
            .prepare_viewport(crate::types::Viewport::new(640, 360, Fixed::ONE))
            .unwrap();
        assert_eq!(scratch.0.get_mut().len(), 640 * 360 * 4);
    }
}
