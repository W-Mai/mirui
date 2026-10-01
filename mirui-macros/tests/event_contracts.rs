#[test]
fn event_body_contracts() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/event_roles.rs");
    cases.compile_fail("tests/ui/event_world_escape.rs");
    cases.compile_fail("tests/ui/event_commands_arguments.rs");
}
