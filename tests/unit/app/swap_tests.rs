use super::*;

#[test]
fn into_backend_reuse_keeps_systems_bounded() {
    let mut app = App::headless(64, 64);
    app.with_default_widgets().with_default_systems();
    let baseline = app.systems.iter().count();
    assert!(baseline > 0);

    for _ in 0..5 {
        let backend = app.into_backend();
        app = App::new(backend);
        app.with_default_widgets().with_default_systems();
        assert_eq!(
            app.systems.iter().count(),
            baseline,
            "rebuilding the App must not accumulate systems",
        );
    }
}

#[test]
fn into_backend_resets_root() {
    let mut app = App::headless(64, 64);
    let root = app.spawn_root().id();
    app.set_root(root);
    assert!(app.root.is_some());

    let backend = app.into_backend();
    let app = App::new(backend);
    assert!(app.root.is_none());
}

#[test]
fn spawned_root_does_not_intercept_pointer_input() {
    let mut app = App::headless(64, 64);
    let root = app.spawn_root().id();
    assert!(app.world.get::<crate::ui::IgnoreHitTest>(root).is_some());
}

#[test]
fn spawned_root_respects_safe_area_by_default() {
    let mut app = App::headless(64, 64);
    let root = app.spawn_root().id();
    assert!(app.world.get::<crate::ui::RespectSafeArea>(root).is_some());
}

#[test]
fn root_can_opt_out_of_safe_area() {
    let mut app = App::headless(64, 64);
    let root = app.spawn_root().ignore_safe_area().id();
    assert!(app.world.get::<crate::ui::RespectSafeArea>(root).is_none());
}

#[test]
fn swap_cycle_leaves_a_rooted_app() {
    use alloc::rc::Rc;
    use core::cell::RefCell;

    let mut first = App::headless(64, 64);
    let root = first.spawn_root().id();
    first.set_root(root);
    let cell = Rc::new(RefCell::new(Some(first)));

    for _ in 0..5 {
        let old = cell.borrow_mut().take().expect("app present before swap");
        let backend = old.into_backend();
        let mut next = App::new(backend);
        let root = next.spawn_root().id();
        next.set_root(root);
        *cell.borrow_mut() = Some(next);

        let guard = cell.borrow();
        let app = guard
            .as_ref()
            .expect("swap must restore the app, never leave None");
        assert!(app.root.is_some(), "rebuilt app must have a root");
    }
}

#[test]
fn path_capacity_changes_preserve_retired_identities_and_enforce_limits() {
    use crate::render::path::{Path, PathStoreError};

    let mut app = App::headless(32, 32);
    app.with_path_capacity(3).unwrap();
    let retired = core::array::from_fn::<_, 3, _>(|_| app.paths().insert(Path::new()).unwrap());
    for id in retired {
        app.paths().remove(id).unwrap();
    }
    for capacity in [0, 1, 3, 1] {
        app.with_path_capacity(capacity).unwrap();
        let mut current = [None; 3];
        for slot in current.iter_mut().take(capacity) {
            *slot = Some(app.paths().insert(Path::new()).unwrap());
        }
        assert_eq!(
            app.paths().insert(Path::new()),
            Err(PathStoreError::Full { capacity }.into())
        );
        for id in retired {
            assert_eq!(
                app.paths().with(id, |_| ()),
                Err(PathStoreError::Missing(id).into())
            );
            assert_eq!(
                app.paths().edit(id, |_| ()),
                Err(PathStoreError::Missing(id).into())
            );
            assert_eq!(
                app.paths().remove(id).unwrap_err(),
                PathStoreError::Missing(id).into()
            );
        }
        for id in current.into_iter().flatten() {
            app.paths().remove(id).unwrap();
        }
    }
}

#[test]
fn path_capacity_cannot_replace_a_live_store() {
    let mut app = App::headless(64, 64);
    let path = app
        .paths()
        .insert(crate::render::path::Path::new())
        .unwrap();

    let result = app.with_path_capacity(8);
    assert!(matches!(
        result,
        Err(crate::render::path::PathStoreError::InUse { paths: 1 })
    ));
    assert!(app.paths().with(path, |_| ()).is_ok());
}
