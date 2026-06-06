#[test]
fn ui_diagnostics_errors_are_stable() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui_diagnostics_fail/*.rs");
}
