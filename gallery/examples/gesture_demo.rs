fn main() {
    gallery::run("mirui - gesture demo", 320, 240, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::gesture::setup_app(setup.app, parent);
        parent
    });
}
