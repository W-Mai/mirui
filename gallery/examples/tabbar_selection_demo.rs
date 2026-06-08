fn main() {
    gallery::run("mirui - tabbar selection events", 480, 200, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::tabbar_selection::setup_app(setup.app, parent);
        parent
    });
}
