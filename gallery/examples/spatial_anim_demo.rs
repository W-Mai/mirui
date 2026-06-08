fn main() {
    gallery::run("mirui - Tween vs Spring vs Elastic", 400, 300, |setup| {
        let parent = mirui::widget::builder::WidgetBuilder::new(&mut setup.app.world).id();
        mirui::gallery::demos::spatial_anim::setup_app(setup.app, parent);
        parent
    });
}
