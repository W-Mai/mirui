fn main() {
    gallery::run("mirui - three body", 480, 320, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::three_body::setup_app(setup.app, parent);
        parent
    });
}
