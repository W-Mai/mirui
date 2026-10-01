#[test]
fn compose_role_contracts() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/compose_roles.rs");
    cases.compile_fail("tests/ui/compose_attribute_arguments.rs");
    cases.compile_fail("tests/ui/compose_invalid_model_role.rs");
    cases.compile_fail("tests/ui/compose_model_pattern.rs");
    cases.compile_fail("tests/ui/compose_projection.rs");
    cases.compile_fail("tests/ui/compose_raw_model_call.rs");
    cases.compile_fail("tests/ui/compose_unsupported_role.rs");
}
