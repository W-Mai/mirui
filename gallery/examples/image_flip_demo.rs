fn main() {
    gallery::run("mirui - 2.5D image flip demo", 480, 320, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::image_flip::setup_app(setup.app, parent);
        parent
    });
}
