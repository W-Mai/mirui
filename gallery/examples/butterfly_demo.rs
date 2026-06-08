fn main() {
    gallery::run("mirui - butterfly", 480, 480, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::butterfly::setup_app(setup.app, parent);
        parent
    });
}
