fn main() {
    gallery::run("mirui - absolute position demo", 480, 320, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::absolute::setup_app(setup.app, parent);
        parent
    });
}
