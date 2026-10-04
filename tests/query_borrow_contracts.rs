#[test]
fn point_fetches_preserve_query_borrow_boundaries() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/query_borrow_contracts/*.rs");
}
