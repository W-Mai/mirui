fn main() {
    gallery::run("mirui - transform demo", 480, 320, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::transform::setup_app(setup.app, parent);
        parent
    });
}
