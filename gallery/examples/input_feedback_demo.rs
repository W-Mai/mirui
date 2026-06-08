fn main() {
    gallery::run("mirui — input feedback", 640, 360, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::input_feedback::setup_app(setup.app, parent);
        parent
    });
}
