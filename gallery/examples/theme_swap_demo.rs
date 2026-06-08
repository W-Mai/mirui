fn main() {
    gallery::run("mirui — theme swap", 480, 320, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::theme_swap::setup_app(setup.app, parent);
        parent
    });
}
