use super::analyze;

fn check(path: &str, source: &str) -> Vec<crate::Finding> {
    analyze(
        path,
        source,
        &[
            "src/app/".into(),
            "src/ui/lifecycle.rs".into(),
            "src/ui/layout/".into(),
        ],
        &["src/app/mod.rs|crate::ui::widgets::text::TextContentError".into()],
    )
}

#[test]
fn widget_implementation_paths_cannot_enter_shared_infrastructure() {
    for source in [
        "fn run() { crate::ui::widgets::follow_bounds::trim_memory(world); }",
        "use crate::ui::widgets::follow_bounds as f; fn run() { f::maintain(world); }",
        "use crate::ui::{widgets::{FollowBounds as F}}; fn run() { world.has::<F>(e); }",
        "use crate::ui as u; fn run() { u::widgets::slider::prepare(world); }",
        "use crate::ui::widgets::*;",
        "use crate::prelude::*;",
        "pub use crate::ui::widgets::follow_bounds::maintain as service;",
        "fn run() { query!(crate::ui::widgets::FollowBounds); }",
        "#[view(component = crate::ui::widgets::FollowBounds)] fn run() {}",
        "fn run() { mirui::ui::widgets::follow_bounds::maintain(world); }",
        "use ::mirui::ui::widgets::FollowBounds;",
        "extern crate self as library; fn run() { library::ui::widgets::follow_bounds::maintain(world); }",
        "use crate::ui::r#widgets::r#follow_bounds as f;",
        "use crate::ui as u; mod nested { use super::u; fn run() { u::widgets::slider::prepare(world); } }",
        "use crate::ui as u; mod nested { fn run() { super::u::widgets::slider::prepare(world); } }",
    ] {
        assert!(!check("src/app/mod.rs", source).is_empty(), "{source}");
    }
}

#[test]
fn relative_paths_resolve_from_the_containing_rust_module() {
    for (path, source) in [
        (
            "src/ui/lifecycle.rs",
            "fn run() { super::widgets::slider::prepare(world); }",
        ),
        (
            "src/ui/layout/paths.rs",
            "use super::super::widgets::FollowBounds;",
        ),
        ("src/ui/layout/mod.rs", "use super::widgets::FollowBounds;"),
        (
            "src/ui/lifecycle.rs",
            "mod inner { use super::super::widgets::FollowBounds; }",
        ),
        (
            "src/ui/lifecycle.rs",
            "use super as u; fn run() { u::widgets::slider::prepare(world); }",
        ),
    ] {
        assert!(!check(path, source).is_empty(), "{path}: {source}");
    }
}

#[test]
fn reviewed_symbols_do_not_authorize_neighbors_or_other_owners() {
    let symbol = "type Error = crate::ui::widgets::text::TextContentError;";
    assert!(check("src/app/mod.rs", symbol).is_empty());
    assert!(!check("src/ui/lifecycle.rs", symbol).is_empty());
    assert!(!check("src/app/mod.rs", "use crate::ui::widgets::text;").is_empty());
    assert!(
        !check(
            "src/app/mod.rs",
            "fn run() { crate::ui::widgets::text::prepare(world); }"
        )
        .is_empty()
    );
}

#[test]
fn tests_comments_and_strings_do_not_create_production_dependencies() {
    assert!(
        check(
            "src/app/mod.rs",
            r#"
        // crate::ui::widgets::follow_bounds::maintain(world);
        const EXAMPLE: &str = "crate::ui::widgets::follow_bounds::maintain(world)";
        #[cfg(test)] mod tests { use crate::ui::widgets::FollowBounds; }
        #[cfg(test)] fn fixture() { crate::ui::widgets::slider::prepare(world); }
        fn run() { println!("crate::ui::widgets::FollowBounds"); }
    "#
        )
        .is_empty()
    );
    assert!(!check("src/app/mod.rs", "#[cfg(any(test, feature = \"std\"))] mod active { use crate::ui::widgets::FollowBounds; }").is_empty());
}

#[test]
fn infrastructure_can_depend_on_generic_layout_and_output_protocols() {
    assert!(
        check(
            "src/ui/layout/paths.rs",
            "use crate::ecs::Entity; use crate::ui::{Outputs, LayoutGeometry};"
        )
        .is_empty()
    );
    assert!(
        check(
            "src/ui/widgets/slider.rs",
            "use crate::ui::widgets::text::Text;"
        )
        .is_empty()
    );
    assert!(
        check(
            "src/ui/render_system.rs",
            "use crate::ui::widgets::follow_bounds;"
        )
        .is_empty()
    );
}

#[test]
fn malformed_protected_source_fails_closed() {
    let findings = check("src/ui/layout/paths.rs", "fn broken(");
    assert_eq!(findings.len(), 1);
    assert!(findings[0].message.contains("parsing failed"));
    let _ = check("src/app/mod.rs", "use nested::nested;");
}

#[test]
fn absolute_fixture_paths_apply_the_same_boundary() {
    assert!(
        !check(
            "/tmp/fixture/src/app/mod.rs",
            "use crate::ui::widgets::FollowBounds;"
        )
        .is_empty()
    );
}
