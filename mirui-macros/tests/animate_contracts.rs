#[test]
fn generated_animation_surface_is_typed() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/animate_roles.rs");
    cases.compile_fail("tests/ui/animate_raw_world.rs");
    cases.compile_fail("tests/ui/animate_non_component.rs");
}
