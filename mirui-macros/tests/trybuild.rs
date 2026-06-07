#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/on_non_tap_with_args.rs");
    t.compile_fail("tests/ui/on_qualified_path.rs");
    t.compile_fail("tests/ui/on_unknown_event.rs");
}
