use metrics::{counter, gauge, histogram, Level};

#[allow(dead_code)]
fn target_only() {
    counter!(target: "rendering", "qwe").increment(1);
    gauge!(target: "rendering", "qwe").set(1.0);
    histogram!(target: "rendering", "qwe").record(1.0);
}

#[allow(dead_code)]
fn level_only() {
    counter!(level: Level::DEBUG, "qwe").increment(1);
    gauge!(level: Level::DEBUG, "qwe").set(1.0);
    histogram!(level: Level::DEBUG, "qwe").record(1.0);
}

#[allow(dead_code)]
fn target_then_level() {
    counter!(target: "rendering", level: Level::DEBUG, "qwe").increment(1);
    gauge!(target: "rendering", level: Level::DEBUG, "qwe").set(1.0);
    histogram!(target: "rendering", level: Level::DEBUG, "qwe").record(1.0);
}

#[allow(dead_code)]
fn level_then_target() {
    counter!(level: Level::DEBUG, target: "rendering", "qwe").increment(1);
    gauge!(level: Level::DEBUG, target: "rendering", "qwe").set(1.0);
    histogram!(level: Level::DEBUG, target: "rendering", "qwe").record(1.0);
}

#[allow(dead_code)]
fn named_args_with_literal_labels() {
    counter!(level: Level::DEBUG, target: "rendering", "qwe", "foo" => "bar").increment(1);
    counter!(target: "rendering", level: Level::DEBUG, "qwe", "foo" => "bar", "baz" => "qux")
        .increment(1);
    counter!(level: Level::DEBUG, "qwe", "foo" => "bar").increment(1);
    gauge!(target: "rendering", "qwe", "foo" => "bar").set(1.0);
    histogram!(level: Level::DEBUG, "qwe", "foo" => "bar").record(1.0);
}

#[allow(dead_code)]
fn named_args_with_expr_labels() {
    let key = "foo";
    let value = String::from("bar");
    counter!(target: "rendering", "qwe", key => value.clone()).increment(1);
    counter!(level: Level::DEBUG, "qwe", key => format!("{}_suffix", value)).increment(1);
    counter!(
        target: "rendering",
        level: Level::DEBUG,
        "qwe",
        key => value.clone(),
        "literal_key" => value.clone()
    )
    .increment(1);
    gauge!(level: Level::DEBUG, target: "rendering", "qwe", key => value.clone()).set(1.0);
    histogram!(target: "rendering", "qwe", key => value).record(1.0);
}

#[allow(dead_code)]
fn named_args_with_inline_slice_labels() {
    counter!(target: "rendering", "qwe", &[("foo", "bar")]).increment(1);
    gauge!(level: Level::DEBUG, target: "rendering", "qwe", &[("foo", "bar")]).set(1.0);
    histogram!(target: "rendering", level: Level::DEBUG, "qwe", &[("foo", "bar")]).record(1.0);
}

#[allow(dead_code)]
fn named_args_with_bound_slice_labels() {
    let labels = [("foo", "bar")];
    counter!(target: "rendering", "qwe", &labels).increment(1);
    counter!(level: Level::DEBUG, target: "rendering", "qwe", &labels).increment(1);

    let owned_labels = vec![(String::from("foo"), String::from("bar"))];
    gauge!(target: "rendering", "qwe", &owned_labels).set(1.0);
    histogram!(level: Level::DEBUG, "qwe", &owned_labels).record(1.0);
}

#[allow(dead_code)]
fn named_args_with_trailing_comma() {
    counter!(target: "rendering", level: Level::DEBUG, "qwe",).increment(1);
    counter!(level: Level::DEBUG, target: "rendering", "qwe", "foo" => "bar",).increment(1);

    let key = "foo";
    let value = "bar";
    counter!(target: "rendering", "qwe", key => value,).increment(1);

    let labels = [("foo", "bar")];
    counter!(level: Level::DEBUG, "qwe", &labels,).increment(1);
}

fn main() {}
