fn main() {
    gallery::run("mirui - DSL demo", 480, 320, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::dsl::setup_app(setup.app, parent);
        parent
    });
}
