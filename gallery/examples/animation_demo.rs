fn main() {
    gallery::run("mirui - animation demo", 320, 180, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::animation::setup_app(setup.app, parent);
        parent
    });
}
