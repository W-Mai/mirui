fn main() {
    gallery::run("mirui - nested scroll", 480, 400, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::nested_scroll::setup_app(setup.app, parent);
        parent
    });
}
