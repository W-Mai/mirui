fn main() {
    let (w, h) = mirui::gallery::demos::effect_glass::DEFAULT_VIEW;
    gallery::run("mirui — effect_glass demo", w, h, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::effect_glass::setup_app(setup.app, parent);
        parent
    });
}
