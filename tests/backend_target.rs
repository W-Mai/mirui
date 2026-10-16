use std::any::Any;
use std::cell::Cell;
use std::rc::Rc;

use mirui::render::texture::{ColorFormat, Texture, TexturePixelsMut};
use mirui::render::{
    BackendTarget, PlaneLayout, RenderError, TargetAlpha, TargetDescriptor, TargetPlan,
};
use mirui::types::Color;

struct Source {
    valid: Rc<Cell<bool>>,
    descriptor: TargetDescriptor,
}

impl BackendTarget for Source {
    fn descriptor(&self) -> TargetDescriptor {
        self.descriptor
    }

    fn allocated_bytes(&self) -> usize {
        8
    }

    fn validate(&self) -> Result<(), RenderError> {
        if self.valid.get() {
            Ok(())
        } else {
            Err(RenderError::BackendFailure)
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn read_pixels_into(&self, dst: &mut TexturePixelsMut<'_>) -> Result<(), RenderError> {
        self.validate()?;
        if dst.width() != 1 || dst.height() != 2 || dst.format() != ColorFormat::RGBA8888 {
            return Err(RenderError::InvalidTexture);
        }
        dst.set_pixel(0, 0, &Color::rgba(80, 160, 240, 128));
        dst.set_pixel(0, 1, &Color::rgb(10, 20, 30));
        Ok(())
    }
}

#[test]
fn external_target_owns_its_source_and_reads_checked_cpu_storage() {
    let valid = Rc::new(Cell::new(true));
    let owner = Rc::downgrade(&valid);
    let descriptor = TargetDescriptor {
        layout: PlaneLayout::packed(1, 2, ColorFormat::RGBA8888).unwrap(),
        alpha: TargetAlpha::Premultiplied,
    };
    let target: Box<dyn BackendTarget> = Box::new(Source { valid, descriptor });
    let plan = TargetPlan {
        descriptor,
        storage_bytes: 8,
        workspace_bytes: 0,
    };
    assert_eq!(target.descriptor(), plan.descriptor);
    assert_eq!(target.allocated_bytes(), plan.storage_bytes);
    assert!(target.as_any().is::<Source>());

    let mut bytes = [0xa5; 16];
    let mut dst = Texture::new(&mut bytes[..], 1, 2, ColorFormat::RGBA8888)
        .unwrap()
        .with_stride(8)
        .unwrap();
    target
        .read_pixels_into(&mut dst.pixels_mut().unwrap())
        .unwrap();
    assert_eq!(
        dst.read_pixels().unwrap().get_pixel(0, 0),
        Color::rgba(80, 160, 240, 128)
    );
    assert_eq!(
        dst.read_pixels().unwrap().get_pixel(0, 1),
        Color::rgb(10, 20, 30)
    );
    assert_eq!(&dst.read_pixels().unwrap().bytes()[4..8], &[0xa5; 4]);

    owner.upgrade().unwrap().set(false);
    assert_eq!(target.validate(), Err(RenderError::BackendFailure));
    assert_eq!(
        target.read_pixels_into(&mut dst.pixels_mut().unwrap()),
        Err(RenderError::BackendFailure)
    );
    drop(target);
    assert!(owner.upgrade().is_none());
}
