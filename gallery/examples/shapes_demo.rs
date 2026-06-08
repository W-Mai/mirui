fn main() {
    gallery::run("mirui - shapes (clock)", 480, 480, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::shapes::setup_app(setup.app, parent);
        parent
    });
}
