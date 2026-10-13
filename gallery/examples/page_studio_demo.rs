fn main() {
    let (width, height) = mirui::gallery::demos::page_studio::DEMO_SIZE
        .fixed_size()
        .expect("Page Studio has a fixed canvas size");
    gallery::run("mirui — Page Studio", width, height, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::page_studio::setup_app(setup.app, parent);
        parent
    });
}
