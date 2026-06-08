fn main() {
    gallery::run("mirui — interactive states", 720, 420, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::interactive_states::setup_app(setup.app, parent);
        parent
    });
}
