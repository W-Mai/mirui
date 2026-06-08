fn main() {
    gallery::run("mirui - components demo", 480, 320, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::components::setup_app(setup.app, parent);
        parent
    });
}
