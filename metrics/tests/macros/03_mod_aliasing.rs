//! This test is to show that we can still use `::metrics::*` macros even though we have imported
//! the `framework::metrics` mod (otherwise, there would be a compilation error).

pub mod framework {
    pub mod metrics {
        pub const UPLOAD_METRIC_NAME: &'static str = "some_metric";
        pub const UPLOAD_METRIC_LABEL_SUCCESS: &'static str = "success";
        pub const UPLOAD_METRIC_LABEL_PROCESS_TYPE: &'static str = "process_type";
    }
}

use framework::*; // This exposes mod `framework::metrics`.

#[inline]
pub fn register_metrics() {
    ::metrics::counter!(
        metrics::UPLOAD_METRIC_NAME,
        &[
            (metrics::UPLOAD_METRIC_LABEL_PROCESS_TYPE, ""),
            (metrics::UPLOAD_METRIC_LABEL_SUCCESS, ""),
        ]
    );
}

fn main() {}
