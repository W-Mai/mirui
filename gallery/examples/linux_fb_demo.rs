//! Smallest possible app on the Linux fbdev backend.
//!
//! `cargo run -p gallery --example linux_fb_demo --features=linux-fb`
//! (Linux only — needs `/dev/fb0` and `/dev/input/event0` permissions
//! or `sudo`).

#[cfg(all(feature = "linux-fb", target_os = "linux"))]
fn main() -> std::io::Result<()> {
    use mirui::app::{App, SwRendererFactory};
    use mirui::prelude::*;
    use mirui::surface::linux;

    // QEMU's `usb-tablet` shows up at `/dev/input/event1` because
    // `usb-kbd` claims `event0`; bare-metal mice usually land on
    // `event0`, so the default config is still right for real boards.
    let cfg = linux::LinuxConfig {
        input_path: Some("/dev/input/event1"),
        ..Default::default()
    };
    let surface = linux::init(cfg)?;
    let mut app = App::with_factory(surface, SwRendererFactory);
    app.with_default_widgets().with_default_systems();
    app.add_plugin(mirui::plugins::input_feedback::InputFeedbackPlugin::new());

    let world = &mut app.world;
    let root = WidgetBuilder::new(world)
        .bg_color(Color::rgb(20, 22, 30))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            padding: Padding::all(24),
            ..Default::default()
        })
        .id();
    ui! {
        :(
            parent: root
            world: world
        :)

        column (direction: FlexDirection::Column, grow: 1.0) {
            title (
                text: "mirui · Linux fbdev",
                bg_color: Color::rgb(38, 50, 70),
                text_color: Color::rgb(220, 220, 240),
                border_radius: 12,
                padding: Padding::all(16)
            ) {}
            spacer (height: 16) {}
            card (
                bg_color: Color::rgb(60, 80, 110),
                border_color: Color::rgb(120, 160, 220),
                border_width: 2,
                border_radius: 16,
                padding: Padding::all(20),
                grow: 1.0
            ) {
                msg (text: "tap to interact", text_color: Color::rgb(240, 240, 255)) {}
            }
        }
    };
    app.set_root(root);
    app.run();
    Ok(())
}

#[cfg(not(all(feature = "linux-fb", target_os = "linux")))]
fn main() {
    eprintln!("linux_fb_demo needs `--features=linux-fb` on a Linux host");
}
