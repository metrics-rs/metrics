use metrics::counter;

#[allow(dead_code)]
fn no_trailing_comma() {
    counter!("qwe", 1);
    counter!(
        "qwe", 1,
        "foo" => "bar"
    );
    counter!("qwe", 1, vec![]);
}

#[allow(dead_code)]
fn with_trailing_comma() {
    counter!("qwe", 1,);
    counter!(
        "qwe", 1,
        "foo" => "bar",
    );
    counter!("qwe", 1, vec![],);
}

fn main() {}
