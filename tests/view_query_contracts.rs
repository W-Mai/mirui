#[test]
fn view_queries_preserve_read_only_borrow_capabilities() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/view_query_contracts/*.rs");
}
