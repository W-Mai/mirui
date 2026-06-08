fn main() {
    gallery::run("LazyList Demo", 320, 320, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::lazy_list::setup_app(setup.app, parent);
        parent
    });
}
