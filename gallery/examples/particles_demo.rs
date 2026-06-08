fn main() {
    gallery::run("mirui - particles", 480, 320, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::particles::setup_app(setup.app, parent);
        parent
    });
}
