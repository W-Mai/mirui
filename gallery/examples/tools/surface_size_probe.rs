#[cfg(all(
    not(target_arch = "wasm32"),
    any(feature = "sdl", feature = "sdl-gpu", feature = "wgpu")
))]
fn main() {
    use mirui::surface::{Surface, SurfaceSizeError};
    use std::time::{Duration, Instant};

    #[cfg(any(feature = "wgpu", feature = "sdl-gpu"))]
    let surface = gallery::ActiveSurface::new("Mirui size control probe", 640, 480).unwrap();
    #[cfg(all(feature = "sdl", not(any(feature = "wgpu", feature = "sdl-gpu"))))]
    let surface = gallery::ActiveSurface::new("Mirui size control probe", 640, 480);
    let mut app = mirui::app::App::with_factory(surface, gallery::ActiveFactory::new());
    app.with_default_widgets().with_default_systems();
    let surface = &mut app.backend;

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
    let root = app.spawn_root().id();
    let state = mirui::gallery::demos::nearby::setup_app(&mut app, root);
    app.tick();
    for target in [(940, 480), (640, 480), (940, 480), (640, 480)] {
        state.toggle_controls();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(!app.tick(), "probe window closed");
            let observed = control.state_untracked();
            if observed.requested.is_none() && observed.actual == target {
                assert_eq!(observed.error, None);
                assert_eq!(app.backend.viewport().logical_size(), target);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Nearby docking stalled: {observed:?}"
            );
        }
        println!("Nearby dock rendered: {target:?}");
    }
    drop(app);
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
