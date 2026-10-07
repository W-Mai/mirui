#[test]
fn composition_resources_keep_borrows_inside_their_access_scope() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/compose_resource_contracts/fail/*.rs");
}
