fn main() {
    let (w, h) = mirui::gallery::demos::offscreen::DEFAULT_VIEW;
    gallery::run(
        "mirui — OffscreenRender demo (auto-toggle every 5s)",
        w,
        h,
        |setup| {
            let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
            mirui::gallery::demos::offscreen::setup_app(setup.app, parent);
            parent
        },
    );
}
