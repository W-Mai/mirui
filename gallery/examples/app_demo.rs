fn main() {
    gallery::run("mirui - app demo", 480, 320, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::app_demo::setup_app(setup.app, parent);
        parent
    });
}
