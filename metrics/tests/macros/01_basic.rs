use metrics::counter;

fn literal_key() {
    counter!("abcdef", 1);
}

fn literal_key_literal_labels() {
    counter!("abcdef", 1, "uvw" => "xyz");
}

fn nonliteral_key() {
    let some_u16 = 0u16;
    counter!(format!("response_status_{}", some_u16), 1);
}

fn nonliteral_key_literal_labels() {
    let some_u16 = 0u16;
    counter!(format!("response_status_{}", some_u16), 1, "uvw" => "xyz");
}

fn nonliteral_key_nonliteral_labels() {
    let some_u16 = 0u16;
    let dynamic_val = "xyz";
    let labels = [("uvw", format!("{}!", dynamic_val))];
    counter!(format!("response_status_{}", some_u16), 12, &labels);
}

fn main() {}
