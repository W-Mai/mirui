fn main() {
    gallery::run("mirui - click demo", 480, 320, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::click::setup_app(setup.app, parent);
        parent
    });
}
