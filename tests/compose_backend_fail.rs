#[test]
fn compose_backend_errors_are_stable() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compose_backend_fail/*.rs");
}
