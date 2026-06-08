fn main() {
    gallery::run("mirui hello", 480, 320, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::hello::setup_app(setup.app, parent);
        parent
    });
}
