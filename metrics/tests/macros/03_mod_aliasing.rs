//! This test is to show that we can still use `::metrics::*` macros even though we have imported
//! the `framework::metrics` mod (otherwise, there would be a compilation error).

pub mod framework {
    pub mod metrics {
        pub struct Key;
        pub struct Label;

        macro_rules! register_counter {
            ($x:expr, $($y:expr),+) => {};
        }
    }
}

use framework::*; // This exposes mod `framework::metrics`.

const UPLOAD_METRIC_NAME: &'static str = "some_metric";
const UPLOAD_METRIC_LABEL_SUCCESS: &'static str = "success";
const UPLOAD_METRIC_LABEL_PROCESS_TYPE: &'static str = "process_type";

#[inline]
pub fn register_metrics() {
    ::metrics::register_counter!(
        UPLOAD_METRIC_NAME,
        &[(UPLOAD_METRIC_LABEL_PROCESS_TYPE, ""), (UPLOAD_METRIC_LABEL_SUCCESS, ""),]
    );
}

fn main() {}
