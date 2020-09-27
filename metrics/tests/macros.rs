#[test]
pub fn macros() {
    let t = trybuild::TestCases::new();
    t.pass("tests/macros/01_trailing_comma.rs");
    t.compile_fail("tests/macros/02_metric_name.rs");
}
