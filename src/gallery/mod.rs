use alloc::vec::Vec;
use core::cell::RefCell;

pub mod demos;
pub(crate) mod play;
pub mod showcase_theme;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DemoSize {
    pub min_width: Option<u16>,
    pub min_height: Option<u16>,
    pub max_width: Option<u16>,
    pub max_height: Option<u16>,
}

impl DemoSize {
    pub const fn constraints(
        min_width: Option<u16>,
        min_height: Option<u16>,
        max_width: Option<u16>,
        max_height: Option<u16>,
    ) -> Self {
        assert!(
            min_width.is_some()
                || min_height.is_some()
                || max_width.is_some()
                || max_height.is_some(),
            "a demo size needs at least one bound"
        );
        if let Some(value) = min_width {
            assert!(value > 0, "minimum width must be positive");
        }
        if let Some(value) = min_height {
            assert!(value > 0, "minimum height must be positive");
        }
        if let Some(value) = max_width {
            assert!(value > 0, "maximum width must be positive");
        }
        if let Some(value) = max_height {
            assert!(value > 0, "maximum height must be positive");
        }
        if let (Some(minimum), Some(maximum)) = (min_width, max_width) {
            assert!(minimum <= maximum, "minimum width exceeds maximum width");
        }
        if let (Some(minimum), Some(maximum)) = (min_height, max_height) {
            assert!(minimum <= maximum, "minimum height exceeds maximum height");
        }
        Self {
            min_width,
            min_height,
            max_width,
            max_height,
        }
    }

    pub const fn fixed(width: u16, height: u16) -> Self {
        Self::constraints(Some(width), Some(height), Some(width), Some(height))
    }

    pub const fn at_most(width: u16, height: u16) -> Self {
        Self::constraints(None, None, Some(width), Some(height))
    }

    pub const fn at_least(width: u16, height: u16) -> Self {
        Self::constraints(Some(width), Some(height), None, None)
    }

    pub const fn range(min_width: u16, min_height: u16, max_width: u16, max_height: u16) -> Self {
        Self::constraints(
            Some(min_width),
            Some(min_height),
            Some(max_width),
            Some(max_height),
        )
    }

    pub const fn fixed_size(self) -> Option<(u16, u16)> {
        match (
            self.min_width,
            self.min_height,
            self.max_width,
            self.max_height,
        ) {
            (Some(min_width), Some(min_height), Some(max_width), Some(max_height))
                if min_width == max_width && min_height == max_height =>
            {
                Some((min_width, min_height))
            }
            _ => None,
        }
    }
}

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
    fn demo_sizes_retain_optional_bounds_and_derive_fixed_canvases() {
        assert_eq!(
            DemoSize::constraints(Some(120), None, Some(640), Some(480)),
            DemoSize {
                min_width: Some(120),
                min_height: None,
                max_width: Some(640),
                max_height: Some(480),
            }
        );
        assert_eq!(DemoSize::fixed(480, 320).fixed_size(), Some((480, 320)));
        assert_eq!(DemoSize::at_most(480, 320).fixed_size(), None);
        assert_eq!(DemoSize::range(320, 240, 1024, 720).fixed_size(), None);
    }

    #[test]
    #[should_panic(expected = "a demo size needs at least one bound")]
    fn demo_size_rejects_an_unbounded_contract() {
        let _ = DemoSize::constraints(None, None, None, None);
    }

    #[test]
    #[should_panic(expected = "minimum width exceeds maximum width")]
    fn demo_size_rejects_an_inverted_width_range() {
        let _ = DemoSize::constraints(Some(481), None, Some(480), None);
    }

    #[test]
    #[should_panic(expected = "maximum height must be positive")]
    fn demo_size_rejects_zero_bounds() {
        let _ = DemoSize::constraints(None, None, None, Some(0));
    }

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
