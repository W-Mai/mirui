fn main() {
    gallery::run("mirui - i18n locale switching", 480, 320, |setup| {
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::i18n::setup_app(setup.app, parent);
        parent
    });
}
