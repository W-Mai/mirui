fn main() {
    gallery::run("mirui - disabled demo", 480, 320, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::disabled::setup_app(setup.app, parent);
        parent
    });
}
