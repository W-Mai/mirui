#[test]
fn component_contracts() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/component_model_roles.rs");
    cases.pass("tests/ui/component_conditional_fields.rs");
    cases.compile_fail("tests/ui/component_arguments.rs");
    cases.compile_fail("tests/ui/component_tuple_model.rs");
    cases.compile_fail("tests/ui/component_enum_model.rs");
    cases.compile_fail("tests/ui/component_projection.rs");
    cases.compile_fail("tests/ui/component_optional_mut.rs");
    cases.compile_fail("tests/ui/component_unsupported_roles.rs");
}

#[test]
fn component_output_contracts() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/component_output_dependencies.rs");
    cases.compile_fail("tests/ui/component_output_source.rs");
    cases.compile_fail("tests/ui/component_output_kind.rs");
    cases.compile_fail("tests/ui/component_output_field_history.rs");
    cases.compile_fail("tests/ui/component_output_conditional_tuple.rs");
}
