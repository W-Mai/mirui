#![cfg(feature = "std")]

use mirui::core::cache::{HasSize, MaxSize};
use mirui::core::resource::{
    FormatMatch, LoadContext, LoadError, LoadInput, Loader, ResourceManager,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::task::{Context, Poll, Wake, Waker};

#[derive(Debug, PartialEq)]
struct Asset(u32);
impl HasSize for Asset {
    fn cache_size(&self) -> usize {
        4
    }
}

struct Completion<T> {
    result: Option<T>,
    wake: Option<Waker>,
}
impl<T> Default for Completion<T> {
    fn default() -> Self {
        Self {
            result: None,
            wake: None,
        }
    }
}
struct ExternalLoader<T>(Arc<Mutex<Completion<T>>>);
impl<T: HasSize + 'static> Loader<T> for ExternalLoader<T> {
    type State = ();
    fn recognize(&self, input: LoadInput<'_>) -> FormatMatch {
        match input {
            LoadInput::Token("remote") => FormatMatch::Match,
            _ => FormatMatch::NoMatch,
        }
    }
    fn poll_load(&self, _: &mut (), cx: &mut LoadContext<'_>) -> Poll<Result<T, LoadError>> {
        let mut completion = self.0.lock().unwrap();
        if let Some(value) = completion.result.take() {
            return Poll::Ready(Ok(value));
        }
        completion.wake = Some(cx.waker().clone());
        Poll::Pending
    }
}

#[derive(Default)]
struct Executor(AtomicUsize);
impl Wake for Executor {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn external_completion_wakes_the_owner_without_publishing_from_the_worker() {
    let completion = Arc::new(Mutex::new(Completion::default()));
    let executor = Arc::new(Executor::default());
    let waker = Waker::from(executor.clone());
    let mut cx = Context::from_waker(&waker);
    let manager = ResourceManager::new(MaxSize::Count(8), Asset(0));
    manager.add_loader(ExternalLoader(completion.clone()));
    let resource = manager.load("remote");
    assert_eq!(resource.get().0, 0);
    manager.poll_pending(&mut cx);
    executor.0.store(0, Ordering::SeqCst);
    std::thread::spawn(move || {
        let wake = {
            let mut completion = completion.lock().unwrap();
            completion.result = Some(Asset(123));
            completion.wake.take().unwrap()
        };
        wake.wake();
    })
    .join()
    .unwrap();
    assert_eq!(executor.0.load(Ordering::SeqCst), 1);
    assert_eq!(resource.get().0, 0);
    manager.poll_pending(&mut cx);
    assert_eq!(resource.get().0, 123);
    executor.0.store(0, Ordering::SeqCst);
    for _ in 0..8 {
        manager.poll_pending(&mut cx);
    }
    assert_eq!(executor.0.load(Ordering::SeqCst), 0);
}

#[cfg(feature = "sync")]
#[test]
fn worker_texture_publication_and_retained_value_handoff_preserve_pixels() {
    use mirui::core::cache::Handle;
    use mirui::render::texture::{ColorFormat, Texture};
    let completion = Arc::new(Mutex::new(Completion::default()));
    let fallback = Texture::from_vec(vec![0; 4], 1, 1, ColorFormat::RGBA8888).unwrap();
    let manager = ResourceManager::new(MaxSize::Count(8), fallback);
    manager.add_loader(ExternalLoader(completion.clone()));
    let resource = manager.load("remote");
    let executor = Arc::new(Executor::default());
    let waker = Waker::from(executor.clone());
    let mut cx = Context::from_waker(&waker);
    drop(resource.get());
    manager.poll_pending(&mut cx);
    let expected = std::thread::spawn(move || {
        let bytes = vec![7, 8, 9, 255];
        let address = bytes.as_ptr() as usize;
        let texture = Texture::from_vec(bytes, 1, 1, ColorFormat::RGBA8888).unwrap();
        let wake = {
            let mut completion = completion.lock().unwrap();
            completion.result = Some(texture);
            completion.wake.take().unwrap()
        };
        wake.wake();
        address
    })
    .join()
    .unwrap();
    assert_ne!(executor.0.load(Ordering::SeqCst), 0);
    assert_ne!(
        resource.get().read_pixels().unwrap().bytes().as_ptr() as usize,
        expected
    );
    manager.poll_pending(&mut cx);
    let value = resource.get();
    assert_eq!(
        value.read_pixels().unwrap().bytes().as_ptr() as usize,
        expected
    );
    assert!(Handle::ptr_eq(&value, &manager.resolve("remote")));
    drop(resource);
    drop(manager);
    let value = std::thread::spawn(move || {
        assert_eq!(value.read_pixels().unwrap().bytes(), &[7, 8, 9, 255]);
        assert_eq!(
            value.read_pixels().unwrap().bytes().as_ptr() as usize,
            expected
        );
        value
    })
    .join()
    .unwrap();
    assert_eq!(
        value.read_pixels().unwrap().bytes().as_ptr() as usize,
        expected
    );
}
