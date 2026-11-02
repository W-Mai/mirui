#[cfg(all(
    not(target_arch = "wasm32"),
    any(feature = "sdl", feature = "sdl-gpu", feature = "wgpu")
))]
fn main() {
    use mirui::surface::{Surface, SurfaceSizeError};
    use std::time::{Duration, Instant};

    #[cfg(feature = "wgpu")]
    let mut surface = gallery::ActiveSurface::new("Mirui size control probe", 640, 480).unwrap();
    #[cfg(all(feature = "sdl-gpu", not(feature = "wgpu")))]
    let mut surface = gallery::ActiveSurface::new("Mirui size control probe", 640, 480).unwrap();
    #[cfg(all(feature = "sdl", not(any(feature = "wgpu", feature = "sdl-gpu"))))]
    let mut surface = gallery::ActiveSurface::new("Mirui size control probe", 640, 480);

    let control = surface
        .size_control()
        .expect("windowed surface size control");
    for target in [(940, 480), (640, 480), (1100, 600), (800, 600), (640, 480)] {
        control.request_size(target.0, target.1).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while control.state_untracked().requested.is_some() {
            while surface.poll_event().is_some() {}
            surface.frame_end().unwrap();
            assert!(
                Instant::now() < deadline,
                "size request stalled: {:?}",
                control.state_untracked()
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        let state = control.state_untracked();
        assert_eq!(state.error, None, "request {target:?}: {state:?}");
        assert_eq!(state.actual, target);
        assert_eq!(surface.viewport().logical_size(), target);
        println!(
            "size accepted: {target:?}, physical={:?}",
            surface.physical_size()
        );
    }
    control.request_size(940, 480).unwrap();
    surface.poll_event();
    control.request_size(1100, 600).unwrap();
    control.request_size(640, 480).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while control.state_untracked().requested.is_some() {
        while surface.poll_event().is_some() {}
        surface.frame_end().unwrap();
        assert!(Instant::now() < deadline, "rapid resize stalled");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(control.state_untracked().actual, (640, 480));
    drop(surface);
    assert_eq!(
        control.request_size(940, 480),
        Err(SurfaceSizeError::Unavailable)
    );
    println!("rapid replacement and owner retirement passed");
}

#[cfg(any(
    target_arch = "wasm32",
    not(any(feature = "sdl", feature = "sdl-gpu", feature = "wgpu"))
))]
fn main() {
    eprintln!("Select a desktop SDL, SDL-GPU or WGPU backend.");
}
