#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/layout_responsive.rs");
    t.pass("tests/ui/cached_if_widget_slot.rs");
    t.compile_fail("tests/ui/layout_container_false.rs");
    t.compile_fail("tests/ui/layout_too_many_dependencies.rs");
    t.compile_fail("tests/ui/layout_undeclared_value.rs");
    t.compile_fail("tests/ui/on_non_tap_with_args.rs");
    t.compile_fail("tests/ui/on_qualified_path.rs");
    t.compile_fail("tests/ui/on_unknown_event.rs");
    t.compile_fail("tests/ui/on_form_a_inside_body.rs");
    t.compile_fail("tests/ui/cached_if_uncontained_reactive.rs");
    t.compile_fail("tests/ui/cached_if_uncontained_slot.rs");
}

#[test]
fn model_contracts() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/model_async_method.rs");
    t.compile_fail("tests/ui/model_borrowed_read.rs");
    t.compile_fail("tests/ui/model_borrowed_write.rs");
    t.compile_fail("tests/ui/model_duplicate_impl.rs");
    t.compile_fail("tests/ui/model_effect_alias_collision.rs");
    t.compile_fail("tests/ui/model_private_observer.rs");
}
