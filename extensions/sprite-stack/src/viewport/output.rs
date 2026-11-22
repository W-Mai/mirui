use super::{
    Attachments, Camera, FrameSize, Resolution, ViewportContent, ViewportOutput, content,
    content_draw::ContentStorage, draw,
};
use crate::raster::{Frame, OverlayFrame};
use alloc::{boxed::Box, rc::Weak};
use core::cell::RefCell;
use mirui::{
    render::{
        RenderError,
        command::{CompositeMode, DrawCommand},
        texture::AlphaMode,
    },
    types::{Fixed, Point},
    ui::view::PaintCtx,
};

enum Storage {
    StageOnly(Box<draw::DefaultRenderer>),
    WithContent(Box<ContentStorage>),
}

struct PreparedFrame {
    stamp: u64,
    viewport: [u16; 2],
    size: FrameSize,
    overlay: bool,
    content: Option<Weak<RefCell<dyn content::FrameSource>>>,
}

impl PreparedFrame {
    fn matches(&self, stamp: u64, size: [u16; 2], attachments: Attachments<'_>) -> bool {
        self.stamp == stamp
            && self.viewport == size
            && self.overlay == attachments.overlay.is_some()
            && match (&self.content, attachments.content) {
                (None, None) => true,
                (Some(identity), Some(content)) => content.matches(identity),
                _ => false,
            }
    }
}

#[derive(Default)]
pub(super) struct Output {
    storage: Option<Storage>,
    content_output: Option<ViewportOutput>,
    frame: Option<Frame>,
    ready: Option<PreparedFrame>,
    texture_revision: u64,
}

impl Output {
    pub(super) fn clear(&mut self) {
        self.ready = None;
        self.frame = None;
        self.storage = None;
        self.content_output = None;
    }

    pub(super) fn paint(
        &mut self,
        source: draw::DrawSource<'_>,
        camera: Camera,
        resolution: Resolution,
        stamp: u64,
        paint: &mut PaintCtx<'_>,
        attachments: Attachments<'_>,
    ) -> Result<(), RenderError> {
        let rect = paint.rect();
        if !rect.w.is_positive() || !rect.h.is_positive() {
            return Ok(());
        }
        let dimension =
            |v: Fixed| u16::try_from(v.ceil().to_int()).map_err(|_| RenderError::InvalidGeometry);
        let size = [dimension(rect.w)?, dimension(rect.h)?];
        if self
            .ready
            .as_ref()
            .is_none_or(|ready| !ready.matches(stamp, size, attachments))
        {
            let frame_size = resolution.fit(size);
            let pixels @ [width, height] = frame_size.pixels().map(usize::from);
            if width == 0 || height == 0 {
                return Err(RenderError::InvalidGeometry);
            }
            self.ready = None;
            let mut content = attachments
                .content
                .filter(|content| content.available())
                .map(|content| content.source.borrow_mut());
            let world = content
                .as_ref()
                .map(|c| c.camera(frame_size).map(|camera| (camera, c.output())))
                .transpose()?;
            let content_output = world.map(|(_, output)| output);
            let new_frame = self.content_output != content_output
                || self
                    .frame
                    .as_ref()
                    .is_none_or(|f| [f.width, f.height] != pixels);
            if new_frame {
                self.frame = None;
                self.storage = None;
            }
            let frame_camera = match world {
                Some((view, _)) => Some(view.camera),
                None if attachments.overlay.is_some() => {
                    Some(draw::DefaultRenderer::frame_camera(camera, frame_size)?)
                }
                None => None,
            };
            let overlay = attachments
                .overlay
                .map(|layer| {
                    layer.prepare(OverlayFrame {
                        size: pixels,
                        camera: frame_camera,
                    })
                })
                .transpose()?
                .flatten();
            if new_frame {
                let mut frame = match content_output {
                    Some(ViewportOutput::Bounded) => {
                        Frame::with_format(width, height, paint.opaque_output_format(2)?)?
                    }
                    _ => Frame::new(width, height)?,
                };
                frame.texture.alpha_mode = if content_output.is_some() {
                    AlphaMode::Opaque
                } else {
                    AlphaMode::Blend
                };
                self.frame = Some(frame);
                self.content_output = content_output;
            }
            let frame = self.frame.as_mut().expect("viewport frame");
            if let Some((view, policy)) = world {
                if !matches!(&self.storage, Some(Storage::WithContent(storage)) if storage.matches(pixels, view.pivot))
                {
                    self.storage = None;
                    self.storage = Some(Storage::WithContent(Box::new(ContentStorage::new(
                        pixels, view.pivot, policy,
                    )?)));
                }
                let Some(Storage::WithContent(storage)) = &mut self.storage else {
                    unreachable!()
                };
                let mut frame = storage.begin(
                    frame_size,
                    view,
                    source,
                    camera.range,
                    overlay.as_deref(),
                    frame,
                );
                content
                    .as_deref_mut()
                    .expect("viewport content")
                    .prepare(&mut frame)?;
            } else {
                let storage = self
                    .storage
                    .get_or_insert_with(|| Storage::StageOnly(Box::default()));
                let Storage::StageOnly(renderer) = storage else {
                    unreachable!()
                };
                renderer.render(source, camera, frame_camera, overlay.as_deref(), frame)?;
            }
            self.texture_revision = self.texture_revision.wrapping_add(1);
            frame.texture.cache_revision = self.texture_revision;
            self.ready = Some(PreparedFrame {
                stamp,
                viewport: size,
                size: frame_size,
                overlay: attachments.overlay.is_some(),
                content: attachments.content.map(ViewportContent::identity),
            });
        }
        let (Some(frame), Some(ready)) = (self.frame.as_ref(), self.ready.as_ref()) else {
            return Ok(());
        };
        let frame_size = ready.size;
        let placement = frame_size.placed(rect);
        let origin = Point::new(placement.x, placement.y);
        let dimensions = Point::new(placement.w, placement.h);
        let scale = frame_size
            .integer_scale()
            .map_or(Fixed::ONE, |scale| Fixed::from(scale.get()));
        if frame.texture.alpha_mode == AlphaMode::Opaque
            && dimensions.x >= rect.w
            && dimensions.y >= rect.h
        {
            paint.set_background_handled(true);
        }
        paint.draw(&DrawCommand::Blit {
            pos: origin,
            size: dimensions,
            transform: paint.transform(),
            quad: None,
            texture: &frame.texture,
            opa: 255,
            radius: Fixed::ZERO,
            composite: CompositeMode::SourceOver,
        });
        if let Some(content) = attachments.content.filter(|content| content.available()) {
            content.source.borrow().present(paint, origin, scale);
        }
        Ok(())
    }
}
