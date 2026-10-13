use super::*;
use crate::app::App;
use crate::core::reactive::{Computed, with_world_scope};
use crate::test_support::tracked_allocations;

#[test]
fn capacity_rejection_and_empty_operations_preserve_path_identity() {
    let mut path = NavigationPath::with_capacity(2);
    path.pop().unwrap();
    path.clear().unwrap();
    assert_eq!(path.revision(), 0);
    path.push(7).unwrap();
    path.push(7).unwrap();
    assert_ne!(path.entries[0].0, path.entries[1].0);
    let entries = path.entries.clone();
    assert_eq!(path.push(8), Err(NavigationError::Capacity { capacity: 2 }));
    assert_eq!(path.entries, entries);
    assert_eq!(path.revision(), 2);
    path.pop().unwrap();
    path.push(7).unwrap();
    assert_eq!(path.entries[0], entries[0]);
    assert_ne!(path.entries[1].0, entries[1].0);
    path.clear().unwrap();
    assert_eq!(path.capacity(), 2);
    assert!(path.is_empty());
    let mut zero = NavigationPath::with_capacity(0);
    assert_eq!(zero.push(1), Err(NavigationError::Capacity { capacity: 0 }));
    assert_eq!(zero.revision(), 0);
}

#[test]
fn exhausted_revisions_never_reuse_entry_identity_or_partially_mutate() {
    let mut path = NavigationPath::with_capacity(2);
    path.push(7).unwrap();
    path.revision = u64::MAX;
    let before = path.entries.clone();
    assert_eq!(path.push(8), Err(NavigationError::RevisionExhausted));
    assert_eq!(path.pop(), Err(NavigationError::RevisionExhausted));
    assert_eq!(path.clear(), Err(NavigationError::RevisionExhausted));
    assert_eq!(path.entries, before);
    assert_eq!(path.revision(), u64::MAX);
}

#[crate::model]
struct Workspace {
    path: model!(owned NavigationPath<u32>),
}

#[crate::model]
impl Workspace {
    #[observe]
    fn depth(&self) -> usize {
        self.path.len()
    }
}

#[test]
fn nested_paths_publish_parent_observations_without_hot_allocation() {
    let mut app = App::headless(32, 32);
    let workspace = app
        .add_model(Workspace {
            path: NavigationPath::with_capacity(2).into(),
        })
        .finish();
    let path = workspace.path();
    let parent = workspace.clone();
    let depth = with_world_scope(&mut app.world, || Computed::new(move || parent.depth()));
    let source = path.clone();
    let revision = with_world_scope(&mut app.world, || Computed::new(move || source.revision()));
    assert_eq!(depth.get(), 0);
    assert_eq!(revision.get(), 0);
    assert_eq!(
        tracked_allocations(|| {
            path.push(7).unwrap();
            assert_eq!(depth.get(), 1);
            path.push(7).unwrap();
            assert_eq!(depth.get(), 2);
            assert!(path.push(8).is_err());
            assert_eq!(revision.get(), 2);
            path.pop().unwrap();
            assert_eq!(depth.get(), 1);
            path.clear().unwrap();
            assert_eq!(depth.get(), 0);
            path.clear().unwrap();
            path.pop().unwrap();
            assert_eq!(revision.get(), 4);
        }),
        0
    );
}

#[test]
fn mutations_do_not_clone_route_payloads() {
    struct Route;
    impl Clone for Route {
        fn clone(&self) -> Self {
            panic!("path mutations must not clone route payloads");
        }
    }
    let mut app = App::headless(32, 32);
    let path = app.add_model(NavigationPath::with_capacity(1)).finish();
    path.push(Route).unwrap();
    assert!(path.push(Route).is_err());
    path.pop().unwrap();
    path.push(Route).unwrap();
    path.clear().unwrap();
}
