fn main() {
    gallery::run("mirui - toggle business events", 480, 240, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::toggle::setup_app(setup.app, parent);
        parent
    });
}
