fn main() {
    gallery::run("mirui — hover tour", 720, 360, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::hover_tour::setup_app(setup.app, parent);
        parent
    });
}
