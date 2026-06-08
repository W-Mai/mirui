fn main() {
    let (w, h) = mirui::gallery::demos::effect::DEFAULT_VIEW;
    gallery::run("mirui — effect widget demo", w, h, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::effect::setup_app(setup.app, parent);
        parent
    });
}
