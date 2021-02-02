#[test]
pub fn macros() {
    let t = trybuild::TestCases::new();
    t.pass("tests/macros/01_basic.rs");
    t.pass("tests/macros/02_trailing_comma.rs");
}
