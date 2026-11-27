use mirui::core::storage::{FileStorage, Storage};
use mirui::prelude::App;
use mirui_first_app::{Chapter, Recipe, install, install_named_recipe};

#[test]
fn every_recipe_mounts_and_draws() {
    for (name, h) in [
        ("reactive", 240),
        ("layout", 240),
        ("spring", 240),
        ("vector", 240),
        ("curve", 230),
        ("scroll", 240),
        ("mini-window", 210),
        ("mini-state", 200),
        ("mini-list", 230),
        ("mini-style", 200),
        ("mini-motion", 220),
        ("mini-storage", 200),
        ("typo-ellipsis", 190),
        ("typo-static", 180),
        ("typo-multiline", 190),
        ("typo-range", 200),
        ("typo-signal", 200),
        ("typo-elastic", 200),
        ("typo-bend", 190),
        ("typo-carets", 200),
        ("editor-shared", 210),
        ("nav-stack", 220),
        ("nav-pageview", 210),
        ("compose-niche", 190),
        ("state-at", 200),
        ("porting-inspector", 220),
        ("events-playground", 220),
        ("controls-gallery", 220),
        ("clipboard-lab", 220),
        ("offscreen-preview", 210),
    ] {
        let w = 290_u16;
        let mut app = App::headless(w, h);
        install_named_recipe(&mut app, Recipe::parse(name).unwrap(), false, None);
        assert!(!app.tick());
        app.render().unwrap();
    }
}

#[test]
fn every_checkpoint_mounts_and_draws() {
    for number in 1..=7 {
        let mut app = App::headless(420, 440);
        let tasks = install(&mut app, Chapter::new(number).unwrap(), false, None);
        if let Some(tasks) = tasks {
            assert_eq!((tasks.completed(), tasks.total()), (1, 3));
            tasks.items().get(0).unwrap().toggle();
            assert_eq!(tasks.completed(), 2);
            assert!(!app.tick());
        }
        app.render().unwrap();
    }
}

#[test]
fn collection_updates_keep_stable_identity_and_restore() {
    let filename = std::env::temp_dir().join(format!("mirui-first-app-{}.bin", std::process::id()));
    let storage = || FileStorage::open(&filename).unwrap().into_handle();
    let mut first = App::headless(420, 440);
    let tasks = install(&mut first, Chapter::FINAL, true, Some(storage())).unwrap();
    let (first_key, first_task) = tasks.items().entries().next().unwrap();
    first_task.toggle();
    tasks.add(Some("Replace the display cable".into()));
    let last_key = tasks.items().entries().next_back().unwrap().0;
    tasks.remove(last_key);
    assert_eq!(tasks.items().entries().next().unwrap().0, first_key);
    first.persistence().save_all().unwrap();
    drop(first);

    let mut restored = App::headless(420, 440);
    let tasks = install(&mut restored, Chapter::FINAL, true, Some(storage())).unwrap();
    assert_eq!((tasks.completed(), tasks.total()), (2, 3));
    restored.render().unwrap();
    std::fs::remove_file(filename).unwrap();
}
