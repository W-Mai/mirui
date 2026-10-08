#[test]
fn authoring_prelude_and_explicit_ecs_boundaries() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/prelude_contracts/explicit_ecs.rs");
    cases.compile_fail("tests/prelude_contracts/default_world.rs");
}

#[test]
fn drawing_prelude_and_explicit_backend_boundaries() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/prelude_contracts/explicit_draw.rs");
    cases.compile_fail("tests/prelude_contracts/default_draw.rs");
}

#[test]
fn layout_callbacks_stay_in_macro_support() {
    let cases = trybuild::TestCases::new();
    cases.pass("mirui-macros/tests/ui/layout_responsive.rs");
    cases.compile_fail("tests/prelude_contracts/raw_layout.rs");
}
