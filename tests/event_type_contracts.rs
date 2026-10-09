#[test]
fn event_types_enforce_source_route_and_result_capabilities() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/event_type_contracts/pass/*.rs");
    cases.compile_fail("tests/event_type_contracts/fail/*.rs");
}

#[test]
fn layout_notifications_are_framework_owned_node_events() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/event_type_contracts/fail/layout_notification_*.rs");
}

#[test]
fn list_updates_enforce_framework_source_and_result_boundaries() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/event_type_contracts/fail/list_update_*.rs");
}
