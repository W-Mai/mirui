fn main() {
    gallery::run("LazyList Demo", 320, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::lazy_list::setup_app(setup.app, parent);
        parent
    });
}
