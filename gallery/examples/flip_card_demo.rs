fn main() {
    gallery::run("mirui - 2.5D flip card demo", 480, 320, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::flip_card::setup_app(setup.app, parent);
        parent
    });
}
