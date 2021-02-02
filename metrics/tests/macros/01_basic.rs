use metrics::counter;

fn static_key() {
    counter!("abcdef", 1);
}

fn dynamic_key() {
    let some_u16 = 0u16;
    counter!(format!("response_status_{}", some_u16), 1);
}

fn main() {}
