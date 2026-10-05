#[test]
fn generated_views_use_explicit_parameter_roles() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/view_explicit_roles.rs");
    cases.compile_fail("tests/ui/view_raw_parameter.rs");
    cases.compile_fail("tests/ui/view_non_component_filter.rs");
    cases.compile_fail("tests/ui/view_duplicate_watch.rs");
    cases.compile_fail("tests/ui/view_empty_roles.rs");
}
