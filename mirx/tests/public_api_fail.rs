#[test]
fn retired_read_apis_remain_inaccessible() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/public_api_fail/*.rs");
}
