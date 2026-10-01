#[test]
fn generated_timer_surface_is_typed_and_schedule_is_exclusive() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/timer_roles.rs");
    cases.compile_fail("tests/ui/timer_raw_world.rs");
    cases.compile_fail("tests/ui/timer_multiple_schedules.rs");
    cases.compile_fail("tests/ui/timer_duplicate_capability.rs");
    cases.compile_fail("tests/ui/timer_optional_model.rs");
}
