//! Native output storage and source-owned pixel access.

use core::any::Any;

use super::renderer::RenderError;
use super::scratch::PlaneLayout;
use super::texture::TexturePixelsMut;

/// Encoding of color channels relative to stored alpha.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetAlpha {
    Straight,
    Premultiplied,
}

/// Checked pixel geometry and native color encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TargetDescriptor {
    pub layout: PlaneLayout,
    pub alpha: TargetAlpha,
}

/// Storage requirements for an independent render target.
///
/// Storage includes native attachments such as multisample and resolve planes. Workspace is additional scratch required while binding and drawing. CPU source copies and readback staging are accounted for separately by resource admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TargetPlan {
    pub descriptor: TargetDescriptor,
    pub storage_bytes: usize,
    pub workspace_bytes: usize,
}

/// Backend-owned storage whose source device remains available independently of a temporary renderer.
///
/// A newly allocated target has no completed contents. Its producer must initialize and submit it successfully before publishing it. Implementations retain the device resources needed for sampling, reading and destruction, without borrowing the producing renderer.
pub trait BackendTarget: Any {
    fn descriptor(&self) -> TargetDescriptor;

    /// Actual native storage allocation, including attachments and padding.
    fn allocated_bytes(&self) -> usize;

    /// Check device and resource validity without allocating, reading pixels or submitting work.
    fn validate(&self) -> Result<(), RenderError>;

    /// Inspect the concrete payload; native consumers must also check device identity and resource generation.
    fn as_any(&self) -> &dyn Any;

    /// Read completed source contents into prepared CPU storage as straight-alpha pixels.
    ///
    /// The destination must match the source dimensions and format. Use its checked stride when writing. Complete source writes before reading and restore any device state on success or error. A failed restoration must prevent further device submissions. Do not allocate another persistent CPU copy or retain the destination borrow.
    fn read_pixels_into(&self, destination: &mut TexturePixelsMut<'_>) -> Result<(), RenderError>;
}
