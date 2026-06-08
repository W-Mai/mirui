fn main() {
    gallery::run("mirui - slider & switch", 320, 200, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::slider_switch::setup_app(setup.app, parent);
        parent
    });
}
