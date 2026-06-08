fn main() {
    let (w, h) = mirui::gallery::demos::cover_flow::DEFAULT_VIEW;
    gallery::run("mirui - cover flow", w, h, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::cover_flow::setup_app(setup.app, parent);
        parent
    });
}
