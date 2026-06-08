fn main() {
    gallery::run("TabBar Demo", 480, 320, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::tabbar::setup_app(setup.app, parent);
        parent
    });
}
