#[test]
fn event_types_enforce_source_route_and_result_capabilities() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/event_type_contracts/pass/*.rs");
    cases.compile_fail("tests/event_type_contracts/fail/*.rs");
}
