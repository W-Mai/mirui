fn main() {
    let (w, h) = mirui::gallery::demos::widgets::DEFAULT_VIEW;
    gallery::run("mirui - widgets", w, h, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::widgets::setup_app(setup.app, parent);
        parent
    });
}
