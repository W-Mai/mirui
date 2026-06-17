//! `cargo run -p gallery --no-default-features --features=linux-drm --example linux_drm_demo`
//!
//! Needs DRM master — stop X/Wayland or run from a console TTY.

#[cfg(all(feature = "linux-drm", target_os = "linux"))]
fn main() {
    gallery::run("mirui linux drm", 800, 600, |setup| {
        let parent = mirui::ui::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::hello::setup_app(setup.app, parent);
        parent
    });
}

#[cfg(not(all(feature = "linux-drm", target_os = "linux")))]
fn main() {
    eprintln!("linux_drm_demo needs `--features=linux-drm` on a Linux host");
}
