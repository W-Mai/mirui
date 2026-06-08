fn main() {
    let (w, h) = mirui::gallery::demos::offscreen_modal::DEFAULT_VIEW;
    gallery::run(
        "mirui — OffscreenRender + WidgetTransform animation (auto-toggle 5s)",
        w,
        h,
        |setup| {
            let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
            mirui::gallery::demos::offscreen_modal::setup_app(setup.app, parent);
            parent
        },
    );
}
