use std::any::Any;
#[path = "support/source_cells.rs"]
pub mod source_cells;
use source_cells::{Cell, Rc};

use mirui::render::texture::Region;
use mirui::render::texture::{ColorFormat, Texture, TexturePixelsMut};
use mirui::render::{
    BackendTarget, PlaneLayout, RenderError, TargetAlpha, TargetDescriptor, TargetPlan,
};
use mirui::types::Color;

#[cfg(all(feature = "std", feature = "sync"))]
#[path = "backend_target/thread_bound.rs"]
mod thread_bound;

struct Source {
    valid: Rc<Cell<bool>>,
    descriptor: TargetDescriptor,
    #[cfg(feature = "sync")]
    storage: Option<mirui::render::target::TargetStorage>,
}

impl BackendTarget for Source {
    #[cfg(feature = "sync")]
    fn retain_storage(
        &mut self,
        storage: mirui::render::target::TargetStorage,
    ) -> Result<(), RenderError> {
        if self.storage.is_some() {
            return Err(RenderError::InvalidScope);
        }
        self.storage = Some(storage);
        Ok(())
    }
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

    fn read_pixels_into(
        &self,
        region: mirui::render::texture::Region,
        dst: &mut TexturePixelsMut<'_>,
    ) -> Result<(), RenderError> {
        let area = self.descriptor().readback_region(region, dst)?;
        self.validate()?;
        for y in 0..area.height() {
            let color = if area.y() + y == 0 {
                Color::rgba(80, 160, 240, 128)
            } else {
                Color::rgb(10, 20, 30)
            };
            dst.set_pixel(0, i32::from(y), &color);
        }
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
    let target: Box<dyn BackendTarget> = Box::new(Source {
        valid,
        descriptor,
        #[cfg(feature = "sync")]
        storage: None,
    });
    let plan = TargetPlan {
        descriptor,
        storage_bytes: 8,
        workspace_bytes: 0,
        readback_bytes: 0,
    };
    assert_eq!(target.descriptor(), plan.descriptor);
    assert_eq!(target.allocated_bytes(), plan.storage_bytes);
    assert!(target.as_any().is::<Source>());

    let mut bytes = [0xa5; 16];
    let mut dst = Texture::new(&mut bytes[..], 1, 2, ColorFormat::RGBA8888)
        .unwrap()
        .with_stride(8)
        .unwrap();
    assert!(dst.backend_target().unwrap().is_none());
    target
        .read_pixels_into(target.descriptor().region(), &mut dst.pixels_mut().unwrap())
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
        target.read_pixels_into(target.descriptor().region(), &mut dst.pixels_mut().unwrap()),
        Err(RenderError::BackendFailure)
    );
    drop(target);
    assert!(owner.upgrade().is_none());
}

#[test]
fn external_region_read_uses_source_coordinates_and_checks_before_writing() {
    let descriptor = TargetDescriptor {
        layout: PlaneLayout::packed(1, 2, ColorFormat::RGBA8888).unwrap(),
        alpha: TargetAlpha::Straight,
    };
    let source = Source {
        valid: Rc::new(Cell::new(true)),
        descriptor,
        #[cfg(feature = "sync")]
        storage: None,
    };
    let mut bytes = [0xa5; 4];
    let mut dst = Texture::new(&mut bytes, 1, 1, ColorFormat::RGBA8888).unwrap();
    source
        .read_pixels_into(
            Region::new(0, 1, 1, 1).unwrap(),
            &mut dst.pixels_mut().unwrap(),
        )
        .unwrap();
    assert_eq!(
        dst.read_pixels().unwrap().get_pixel(0, 0),
        Color::rgb(10, 20, 30)
    );
    for area in [
        Region::new(0, 2, 1, 1).unwrap(),
        Region::new(0, 0, 1, 2).unwrap(),
        Region::new(0, 0, 0, 0).unwrap(),
        Region::new(u32::MAX, 0, 0, 1).unwrap(),
    ] {
        dst.pixels_mut().unwrap().bytes_mut().fill(0xa5);
        assert_eq!(
            source.read_pixels_into(area, &mut dst.pixels_mut().unwrap()),
            Err(RenderError::InvalidTexture)
        );
        assert_eq!(dst.read_pixels().unwrap().bytes(), &[0xa5; 4]);
    }
}
