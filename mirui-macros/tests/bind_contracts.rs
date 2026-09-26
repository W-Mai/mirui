#[test]
fn invalid_bound_declarations() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/bind_raw_model_call.rs");
    cases.compile_fail("tests/ui/bind_unsupported_type.rs");
}
