use bolero::fuzz;
use metrics_util::StreamingIntegers;

fn main() {
    fuzz!().with_type().for_each(|value: &Vec<u64>| {
        let mut si = StreamingIntegers::new();
        si.compress(&value);
    });
}
