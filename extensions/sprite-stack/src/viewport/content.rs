use super::{FrameSize, ObjectPass};
use crate::{
    StackScene,
    raster::{
        CameraWorkspace, Frame, FrameCamera, FrameEffects, FrameOverlay, RollMapping, SceneLayers,
    },
};
use alloc::rc::{Rc, Weak};
use core::cell::RefCell;
use mirui::{
    render::RenderError,
    types::{Fixed, Point},
    ui::view::PaintCtx,
};

/// A camera and the output-space pivot of its roll.
#[derive(Clone, Copy)]
pub struct WorldCamera {
    pub camera: FrameCamera,
    pub pivot: Fixed,
    /// Enable roll mapping during a transition even when the angle passes through zero.
    pub roll: bool,
}

/// Storage policy for opaque viewport content.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WorldOutput {
    #[default]
    Full,
    /// Tile source color and depth and use a two-byte opaque output format.
    Bounded,
}

/// Supplies camera and layer data for a viewport. SpriteStack owns the frame,
/// projection mapping, tile order and presentation.
pub trait SceneContent: 'static {
    fn camera(&self, size: FrameSize) -> Result<WorldCamera, RenderError>;

    fn output(&self) -> WorldOutput {
        WorldOutput::Full
    }

    /// Prepares transient scene data and submits its layers exactly once.
    fn prepare(
        &mut self,
        scene: &StackScene,
        frame: &mut WorldFrame<'_>,
    ) -> Result<(), RenderError>;

    /// Paints effects at display resolution after the viewport image.
    fn present(&self, _paint: &mut PaintCtx<'_>, _origin: Point, _scale: Fixed) {}
}

/// Attaches procedural scene content to a StackViewport using the component list. Changes are published through the viewport's StackScene.
#[mirui::component]
pub struct ViewportContent {
    pub(super) source: Rc<RefCell<dyn SceneContent>>,
}

impl ViewportContent {
    pub fn new(content: impl SceneContent) -> Self {
        Self {
            source: Rc::new(RefCell::new(content)),
        }
    }

    pub(super) fn identity(&self) -> Weak<RefCell<dyn SceneContent>> {
        Rc::downgrade(&self.source)
    }

    pub(super) fn matches(&self, identity: &Weak<RefCell<dyn SceneContent>>) -> bool {
        core::ptr::addr_eq(identity.as_ptr(), Rc::as_ptr(&self.source))
    }
}

pub(super) struct WorldStorage {
    workspace: CameraWorkspace,
    objects: ObjectPass,
    size: [usize; 2],
    pivot: Fixed,
}

impl WorldStorage {
    pub(super) fn new(
        size: [usize; 2],
        pivot: Fixed,
        output: WorldOutput,
    ) -> Result<Self, RenderError> {
        Ok(Self {
            workspace: CameraWorkspace::new(size, pivot, output == WorldOutput::Bounded)?,
            objects: ObjectPass::default(),
            size,
            pivot,
        })
    }

    pub(super) fn matches(&self, size: [usize; 2], pivot: Fixed) -> bool {
        self.size == size && self.pivot == pivot
    }

    pub(super) fn begin<'a>(
        &'a mut self,
        size: FrameSize,
        view: WorldCamera,
        overlay: Option<&'a dyn FrameOverlay>,
        output: &'a mut Frame,
    ) -> WorldFrame<'a> {
        let mut camera = view.camera;
        let rotation = (view.roll || camera.roll != Fixed::ZERO)
            .then(|| (Fixed::sin_rad(camera.roll), Fixed::cos_rad(camera.roll)));
        let mapping = RollMapping::new(self.size, view.pivot, rotation);
        if mapping.rolled {
            let axis = Fixed::from_int(self.size[0] as i32) / 2;
            camera.projection.center =
                mapping.projection_center([axis, camera.projection.center[1]]);
        }
        WorldFrame {
            size,
            camera,
            mapping,
            overlay,
            output,
            workspace: &mut self.workspace,
            objects: &mut self.objects,
            submitted: false,
        }
    }
}

/// Inputs and reusable object preparation for one frame.
pub struct WorldFrame<'a> {
    size: FrameSize,
    camera: FrameCamera,
    mapping: RollMapping,
    overlay: Option<&'a dyn FrameOverlay>,
    output: &'a mut Frame,
    workspace: &'a mut CameraWorkspace,
    objects: &'a mut ObjectPass,
    submitted: bool,
}

impl WorldFrame<'_> {
    pub fn size(&self) -> FrameSize {
        self.size
    }

    /// Camera in source coordinates after roll mapping.
    pub fn camera(&self) -> FrameCamera {
        self.camera
    }

    pub fn source_size(&self) -> [usize; 2] {
        self.mapping.size
    }

    /// Bytes allocated for viewport pixels and reusable scene work.
    pub fn storage_bytes(&self) -> usize {
        core::mem::size_of::<WorldStorage>()
            + self.workspace.bytes()
            + self.objects.storage_bytes()
            + self.output.bytes()
    }

    pub(super) fn finish(self) -> Result<(), RenderError> {
        if self.submitted {
            Ok(())
        } else {
            Err(RenderError::InvalidGeometry)
        }
    }

    /// Split object preparation from the exclusive layer submission.
    pub fn layers(&mut self) -> (WorldLayers<'_>, &mut ObjectPass) {
        (
            WorldLayers {
                mapping: &self.mapping,
                overlay: self.overlay,
                output: self.output,
                workspace: self.workspace,
                submitted: &mut self.submitted,
            },
            self.objects,
        )
    }
}

/// Writes prepared layers once with the viewport's tile and depth passes.
pub struct WorldLayers<'a> {
    mapping: &'a RollMapping,
    overlay: Option<&'a dyn FrameOverlay>,
    output: &'a mut Frame,
    workspace: &'a mut CameraWorkspace,
    submitted: &'a mut bool,
}

impl WorldLayers<'_> {
    pub fn draw(
        self,
        layers: &impl SceneLayers,
        effects: &mut impl FrameEffects,
    ) -> Result<(), RenderError> {
        if *self.submitted {
            return Err(RenderError::InvalidGeometry);
        }
        self.workspace
            .render_layers(self.output, self.mapping, self.overlay, layers, effects)?;
        *self.submitted = true;
        Ok(())
    }
}
