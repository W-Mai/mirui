fn main() {
    gallery::run("mirui - subpixel motion", 480, 320, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::subpixel::setup_app(setup.app, parent);
        parent
    });
}
