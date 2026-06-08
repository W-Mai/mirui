//! Runs the shared `hello` scene on the Linux fbdev backend.
//!
//! `cargo run -p gallery --example linux_fb_demo --features=linux-fb`
//! (Linux only — needs `/dev/fb0` and `/dev/input/event*`
//! permissions or `sudo`).

#[cfg(all(feature = "linux-fb", target_os = "linux"))]
fn main() {
    gallery::run("mirui linux fbdev", 800, 600, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::hello::setup_app(setup.app, parent);
        parent
    });
}

#[cfg(not(all(feature = "linux-fb", target_os = "linux")))]
fn main() {
    eprintln!("linux_fb_demo needs `--features=linux-fb` on a Linux host");
}
