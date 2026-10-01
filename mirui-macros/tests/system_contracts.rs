#[test]
fn generated_system_internals_are_not_visible_to_authored_code() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/system_world_escape.rs");
    cases.compile_fail("tests/ui/system_raw_constructor.rs");
    cases.compile_fail("tests/ui/system_macro_constructor.rs");
    cases.compile_fail("tests/ui/system_priority_mutation.rs");
    cases.compile_fail("tests/ui/system_duplicate_binding.rs");
    cases.compile_fail("tests/ui/system_parameter_attribute.rs");
    cases.pass("tests/ui/system_relative_paths.rs");
}
