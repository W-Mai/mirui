use mirui::core::storage::{FileStorage, Storage};
use mirui::prelude::App;
use mirui_first_app::{Chapter, Recipe, install, install_named_recipe};

#[test]
fn every_recipe_mounts_and_draws() {
    for name in [
        "reactive",
        "layout",
        "spring",
        "vector",
        "curve",
        "scroll",
        "mini-window",
        "mini-state",
        "mini-list",
        "mini-style",
        "mini-motion",
        "mini-storage",
        "typo-ellipsis",
        "typo-static",
        "typo-multiline",
        "typo-range",
        "typo-signal",
        "typo-elastic",
        "typo-bend",
        "typo-carets",
        "editor-shared",
        "nav-stack",
        "nav-pageview",
        "compose-niche",
        "state-at",
    ] {
        let mut app = App::headless(420, 260);
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
