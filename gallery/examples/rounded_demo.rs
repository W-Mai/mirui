fn main() {
    gallery::run("mirui - rounded + border", 480, 320, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::rounded::setup_app(setup.app, parent);
        parent
    });
}
