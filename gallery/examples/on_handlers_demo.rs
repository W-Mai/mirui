fn main() {
    gallery::run("mirui - on handlers", 640, 320, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::on_handlers::setup_app(setup.app, parent);
        parent
    });
}
