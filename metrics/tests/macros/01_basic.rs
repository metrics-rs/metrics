use metrics::{counter, describe_counter, Unit};

#[allow(dead_code)]
fn literal_key() {
    describe_counter!("abcdef", "a counter");
    describe_counter!("abcdef", Unit::Nanoseconds, "a counter");
    let _ = counter!("abcdef");
    counter!("abcdef").increment(1);
}

#[allow(dead_code)]
fn literal_key_literal_labels() {
    describe_counter!("abcdef", "a counter");
    describe_counter!("abcdef", Unit::Nanoseconds, "a counter");
    let _ = counter!("abcdef", "uvw" => "xyz");
    counter!("abcdef", "uvw" => "xyz").increment(1);
}

#[allow(dead_code)]
fn nonliteral_key() {
    let some_u16 = 0u16;
    describe_counter!(format!("response_status_{}", some_u16), "a counter");
    describe_counter!(format!("response_status_{}", some_u16), Unit::Nanoseconds, "a counter");
    let _ = counter!(format!("response_status_{}", some_u16));
    counter!(format!("response_status_{}", some_u16)).increment(1);
}

#[allow(dead_code)]
fn nonliteral_key_literal_labels() {
    let some_u16 = 0u16;
    describe_counter!(format!("response_status_{}", some_u16), "a counter");
    describe_counter!(format!("response_status_{}", some_u16), Unit::Nanoseconds, "a counter");
    let _ = counter!(format!("response_status_{}", some_u16), "uvw" => "xyz");
    counter!(format!("response_status_{}", some_u16), "uvw" => "xyz").increment(1);
}

#[allow(dead_code)]
fn nonliteral_key_nonliteral_labels() {
    let some_u16 = 0u16;
    let dynamic_val = "xyz";
    let labels = [("uvw", format!("{}!", dynamic_val))];
    describe_counter!(format!("response_status_{}", some_u16), "a counter");
    describe_counter!(format!("response_status_{}", some_u16), Unit::Nanoseconds, "a counter");
    let _ = counter!(format!("response_status_{}", some_u16), &labels);
    counter!(format!("response_status_{}", some_u16), &labels).increment(12);
}

#[allow(dead_code)]
fn const_key() {
    const KEY: &str = "abcdef";
    describe_counter!(KEY, "a counter");
    describe_counter!(KEY, Unit::Nanoseconds, "a counter");
    let _ = counter!(KEY);
    counter!(KEY).increment(17);
}

#[allow(dead_code)]
fn const_description() {
    const DESC: &str = "a counter";
    describe_counter!("abcdef", DESC);
    describe_counter!("abcdef", Unit::Nanoseconds, DESC);
}

fn main() {}
