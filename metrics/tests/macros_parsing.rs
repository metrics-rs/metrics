#[test]
pub fn parsing() {
    let t = trybuild::TestCases::new();
    t.pass("tests/macros_parsing/*.rs");
}
