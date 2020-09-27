use metrics::counter;

fn no_trailing_comma() {
    counter!("qwe", 1);
    counter!(
        "qwe", 1,
        "foo" => "bar"
    );
    counter!("qwe", 1, vec![]);
}

fn with_trailing_comma() {
    counter!("qwe", 1,);
    counter!(
        "qwe", 1,
        "foo" => "bar",
    );
    counter!("qwe", 1, vec![],);
}

fn main() {}
