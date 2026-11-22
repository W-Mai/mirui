//! Renderer trait → Scene capture. Sits alongside other backends because it
//! satisfies the same trait the screen renderers do, even though the "target"
//! is an op stream rather than pixels.

use alloc::vec::Vec;

use crate::render::command::DrawCommand;
use crate::render::renderer::{DrawRequest, RenderError, RenderFeature, RenderRoute, Renderer};
use crate::render::scene::Scene;
use crate::render::scene::record::{RecordError, ResourceResolver, record_command};

/// Records checked draw requests into a scene.
pub struct SceneRenderer<'a> {
    pub scene: &'a mut Scene,
    pub resolver: &'a mut dyn ResourceResolver,
    pub errors: Vec<RecordError>,
}

impl<'a> SceneRenderer<'a> {
    pub fn new(scene: &'a mut Scene, resolver: &'a mut dyn ResourceResolver) -> Self {
        Self {
            scene,
            resolver,
            errors: Vec::new(),
        }
    }
}

impl Renderer for SceneRenderer<'_> {
    type TargetState = ();

    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        request.validate_projection()?;
        request.validate_texture()?;
        if !request.projective.is_identity() {
            return Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry));
        }
        if matches!(request.command, DrawCommand::ApplyBlur { .. }) {
            return Err(RenderError::Unsupported(RenderFeature::Blur));
        }
        Ok(RenderRoute::Native)
    }

    fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
        let request = &draw.request();
        self.route(request)?;
        match record_command(request.command, self.resolver) {
            Ok(op) => {
                self.scene.push(op);
                Ok(())
            }
            Err(error) => {
                self.errors.push(error);
                Err(RenderError::BackendFailure)
            }
        }
    }

    fn prepare<'data>(
        &mut self,
        request: DrawRequest<'_, 'data>,
        route: RenderRoute,
        _: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Result<crate::render::PreparedDraw<'data>, RenderError>> {
        // Recording retains resource references, not sampled pixels.
        core::task::Poll::Ready(crate::render::PreparedDraw::native(
            request,
            route,
            self.output_scale(),
        ))
    }

    fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
        Ok(())
    }
}

impl Scene {
    pub fn renderer<'a>(&'a mut self, resolver: &'a mut dyn ResourceResolver) -> SceneRenderer<'a> {
        SceneRenderer::new(self, resolver)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/render/backends/scene.rs"]
mod tests;
