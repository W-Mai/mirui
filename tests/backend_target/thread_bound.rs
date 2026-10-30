use super::*;
use mirui::render::target::{TargetStorage, ThreadOwner, ThreadResource};

struct Local {
    source: Source,
    drops: std::rc::Rc<Cell<usize>>,
    thread: std::thread::ThreadId,
}

impl Drop for Local {
    fn drop(&mut self) {
        assert_eq!(self.thread, std::thread::current().id());
        self.drops.set(self.drops.get() + 1);
    }
}

struct ThreadSource {
    local: ThreadResource<Local>,
    descriptor: TargetDescriptor,
    bytes: usize,
}

impl BackendTarget for ThreadSource {
    fn descriptor(&self) -> TargetDescriptor {
        self.descriptor
    }
    fn allocated_bytes(&self) -> usize {
        self.bytes
    }
    fn retain_storage(&mut self, storage: TargetStorage) -> Result<(), RenderError> {
        self.local.retain_storage(storage)
    }
    fn validate(&self) -> Result<(), RenderError> {
        self.local.validate()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn read_pixels_into(
        &self,
        region: Region,
        dst: &mut TexturePixelsMut<'_>,
    ) -> Result<(), RenderError> {
        self.descriptor.readback_region(region, dst)?;
        self.local
            .with(|local| local.source.read_pixels_into(region, dst))
    }
}

fn source(owner: &mut ThreadOwner, drops: &std::rc::Rc<Cell<usize>>) -> Box<dyn BackendTarget> {
    let descriptor = TargetDescriptor {
        layout: PlaneLayout::packed(1, 2, ColorFormat::RGBA8888).unwrap(),
        alpha: TargetAlpha::Straight,
    };
    Box::new(ThreadSource {
        bytes: owner.metadata_bytes::<Local>().unwrap() + 8,
        local: owner
            .insert(Local {
                source: Source {
                    descriptor,
                    valid: Rc::new(Cell::new(true)),
                    storage: None,
                },
                drops: drops.clone(),
                thread: std::thread::current().id(),
            })
            .unwrap(),
        descriptor,
    })
}

fn read(target: &dyn BackendTarget) -> Result<Color, RenderError> {
    let mut bytes = [0; 8];
    let mut image = Texture::new(&mut bytes, 1, 2, ColorFormat::RGBA8888)?;
    target.read_pixels_into(target.descriptor().region(), &mut image.pixels_mut()?)?;
    Ok(image.read_pixels()?.get_pixel(0, 1))
}

#[test]
fn external_thread_bound_target_checks_reads_and_retires_on_its_owner() {
    let drops = std::rc::Rc::new(Cell::new(0));
    let mut owner = ThreadOwner::new();
    let target = source(&mut owner, &drops);
    let planned = owner.metadata_bytes::<Local>().unwrap() + 8;
    assert_eq!(target.allocated_bytes(), planned);
    assert_eq!(read(&*target), Ok(Color::rgb(10, 20, 30)));
    let target = std::thread::spawn(move || {
        assert_eq!(target.validate(), Ok(()));
        assert_eq!(read(&*target), Err(RenderError::WrongThread));
        target
    })
    .join()
    .unwrap();
    assert_eq!(read(&*target), Ok(Color::rgb(10, 20, 30)));
    std::thread::spawn(move || drop(target)).join().unwrap();
    assert_eq!(drops.get(), 0);
    owner.collect().unwrap();
    assert_eq!(drops.get(), 1);
    owner.collect().unwrap();
    assert_eq!(drops.get(), 1);
}

#[test]
fn external_closed_owner_destroys_raw_before_late_worker_release() {
    let drops = std::rc::Rc::new(Cell::new(0));
    let mut owner = ThreadOwner::new();
    let target = source(&mut owner, &drops);
    owner.close().unwrap();
    assert_eq!(drops.get(), 1);
    std::thread::spawn(move || {
        assert_eq!(target.validate(), Err(RenderError::InvalidTexture));
        assert_eq!(read(&*target), Err(RenderError::InvalidTexture));
        drop(target);
    })
    .join()
    .unwrap();
    assert_eq!(owner.collect(), Err(RenderError::InvalidTexture));
    assert_eq!(drops.get(), 1);
}
