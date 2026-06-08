fn main() {
    gallery::run("mirui - scroll demo", 480, 320, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::scroll::setup_app(setup.app, parent);
        parent
    });
}
