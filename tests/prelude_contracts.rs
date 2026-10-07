#[test]
fn authoring_prelude_and_explicit_ecs_boundaries() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/prelude_contracts/explicit_ecs.rs");
    cases.compile_fail("tests/prelude_contracts/default_world.rs");
}
