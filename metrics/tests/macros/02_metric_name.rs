use metrics::counter;

fn valid_name() {
    counter!("abc_def", 1);
}

fn invalid_name() {
    counter!("abc$def");
}

fn main() {}
