fn main() {
    gallery::run("mirui - book flip (transform-origin)", 640, 360, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::book_flip::setup_app(setup.app, parent);
        parent
    });
}
