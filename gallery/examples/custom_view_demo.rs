fn main() {
    gallery::run("custom_view_demo", 480, 200, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::custom_view::setup_app(setup.app, parent);
        parent
    });
}
