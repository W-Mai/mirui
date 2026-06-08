fn main() {
    gallery::run("TextInput Demo", 480, 200, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::text_input::setup_app(setup.app, parent);
        parent
    });
}
