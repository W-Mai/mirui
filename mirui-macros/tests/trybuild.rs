#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/layout_responsive.rs");
    t.compile_fail("tests/ui/layout_container_false.rs");
    t.compile_fail("tests/ui/layout_too_many_dependencies.rs");
    t.compile_fail("tests/ui/layout_undeclared_value.rs");
    t.compile_fail("tests/ui/on_non_tap_with_args.rs");
    t.compile_fail("tests/ui/on_qualified_path.rs");
    t.compile_fail("tests/ui/on_unknown_event.rs");
    t.compile_fail("tests/ui/on_form_a_inside_body.rs");
}
