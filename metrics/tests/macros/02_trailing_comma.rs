use metrics::counter;

#[allow(dead_code)]
fn no_trailing_comma() {
    counter!("qwe").increment(1);
    counter!(
        "qwe",
        "foo" => "bar"
    ).increment(1);
    counter!("qwe", vec![]).increment(1);
}

#[allow(dead_code)]
fn with_trailing_comma() {
    counter!("qwe",);
    counter!(
        "qwe", 
        "foo" => "bar",
    ).increment(1);
    counter!("qwe", vec![],).increment(1);
}

fn main() {}
