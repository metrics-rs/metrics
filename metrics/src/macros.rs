/// Increments a counter by a value.
///
/// This will register a counter with the given name, if it does not already
/// exist, then increment it by the given value. Optionally, a set of labels,
/// of the form `key => value`, can be passed to further describe the counter.
///
/// Functionally equivalent to calling [`Recorder::increment_counter`].
///
/// ### Examples
///
/// ```rust
/// use metrics::counter;
///
/// fn send_msg() {
///     counter!("msg_sent_total", 1);
///     // ...
/// }
/// # fn main() {}
/// ```
///
/// Labels can also be optionally provided.
///
/// ```rust
/// use metrics::counter;
///
/// fn do_thing() {
///     let count: u64 = 42;
///     let user: String = String::from("jane");
///     counter!("do_thing", count, "service" => "admin", "user" => user);
/// }
/// # fn main() {}
/// ```
#[macro_export]
macro_rules! counter {
    ($name:expr, $value:expr) => {
        if let Some(recorder) = $crate::try_recorder() {
            recorder.increment_counter($crate::Key::from_name($name), $value);
        }
    };

    ($name:expr, $value:expr, $($labels:tt)*) => {
        if let Some(recorder) = $crate::try_recorder() {
            let labels = $crate::labels!( $($labels)* );
            let key = $crate::Key::from_name_and_labels($name, labels);
            recorder.increment_counter(key, $value);
        }
    };
}

/// Update a gauge with a value.
///
/// This will register a gauge with the given name, if it does not already
/// exist, then update it, replacing the previous value with given value. Optionally,
/// a set of labels, of the form `key => value`, can be passed to further
/// describe the gauge.
///
/// Functionally equivalent to calling [`Recorder::update_gauge`].
///
/// ### Examples
///
/// ```rust
/// use metrics::gauge;
///
/// fn update_current_value() {
///     gauge!("current_value", -131);
/// }
/// # fn main() {}
/// ```
///
/// Labels can also be passed along:
///
/// ```rust
/// use metrics::gauge;
///
/// fn update_current_value() {
///     let value: i64 = -131;
///     let creator: String = String::from("jane");
///     gauge!("current_value", value, "creator" => creator);
/// }
/// # fn main() {}
/// ```
#[macro_export]
macro_rules! gauge {
    ($name:expr, $value:expr) => {
        if let Some(recorder) = $crate::try_recorder() {
            $crate::__private_api_update_gauge(recorder, $crate::Key::from_name($name), $value);
        }
    };

    ($name:expr, $value:expr, $($labels:tt)*) => {
        if let Some(recorder) = $crate::try_recorder() {
            let labels = $crate::labels!( $($labels)* );
            let key = $crate::Key::from_name_and_labels($name, labels);
            $crate::__private_api_update_gauge(recorder, key, $value);
        }
    };
}

/// Records a timing.
///
/// This will register an histogram with the given name, if it does not already
/// exist, then add data point with the given timing. This timing must implement
/// [`AsNanoseconds`]. Optionally, a set of labels, of the form `key => value`,
/// can be passed to further describe the histogram.
///
/// Functionally equivalent to calling [`Recorder::record_histogram`].
///
/// ### Examples
///
/// ```rust
/// use metrics::timing;
/// use std::time::Instant;
///
/// # fn process() {}
/// fn handle_request() {
///     let start = Instant::now();
///     process();
///     let end = Instant::now();
///
///     // We can pass instances of `Instant` directly:
///     timing!("perf.request_processed", start, end);
///
///     // Or we can pass just the delta:
///     let delta = end - start;
///     timing!("perf.request_processed", delta);
///
///     // And we can even pass unsigned values, both for the start/end notation:
///     let start: u64 = 100;
///     let end: u64 = 200;
///     timing!("perf.request_processed", start, end);
///
///     // And the delta notation:
///     let delta: u64 = end - start;
///     timing!("perf.request_processed", delta);
/// }
/// # fn main() {}
/// ```
///
/// Labels can also be passed along:
///
/// ```rust
/// use metrics::timing;
/// use std::time::Instant;
///
/// # fn process() {}
/// fn handle_request() {
///     let start = Instant::now();
///     process();
///     let end = Instant::now();
///
///     // We can pass instances of `Instant` directly:
///     timing!("perf.request_processed", start, end, "service" => "http", "type" => "checkout");
///
///     // Or we can pass just the delta:
///     let delta = end - start;
///     timing!("perf.request_processed", delta, "service" => "http", "type" => "checkout");
///
///     // And we can even pass unsigned values, both for the start/end notation:
///     let start: u64 = 100;
///     let end: u64 = 200;
///     timing!("perf.request_processed", start, end, "service" => "http", "type" => "checkout");
///
///     // And the delta notation:
///     let delta: u64 = end - start;
///     timing!("perf.request_processed", delta, "service" => "http", "type" => "checkout");
/// }
/// # fn main() {}
/// ```
///
/// [`AsNanoseconds`]: https://docs.rs/metrics-core/0.5/metrics_core/trait.AsNanoseconds.html
#[macro_export]
macro_rules! timing {
    ($name:expr, $value:expr) => {
        if let Some(recorder) = $crate::try_recorder() {
            $crate::__private_api_record_histogram(recorder, $crate::Key::from_name($name), $value);
        }
    };

    ($name:expr, $start:expr, $end:expr) => {
        $crate::timing!($name, $end - $start)
    };

    ($name:expr, $start:expr, $end:expr, $($labels:tt)*) => {
        $crate::timing!($name, $end - $start, $($labels)*)
    };

    ($name:expr, $value:expr, $($labels:tt)*) => {
        if let Some(recorder) = $crate::try_recorder() {
            let labels = $crate::labels!( $($labels)* );
            let key = $crate::Key::from_name_and_labels($name, labels);
            $crate::__private_api_record_histogram(recorder, key, $value);
        }
    };
}

/// Records a value.
///
/// This will register an histogram with the given name, if it does not already
/// exist, then add data point with the given value. Optionally, a set of labels,
/// of the form `key => value`, can be passed to further describe the histogram.
///
/// Functionally equivalent to calling [`Recorder::record_histogram`].
///
/// ### Examples
///
/// ```rust
/// use metrics::value;
///
/// # fn process() -> u64 { 42 }
/// fn handle_request() {
///     let rows_read = process();
///     value!("client.process_num_rows", rows_read);
/// }
/// # fn main() {}
/// ```
///
/// Labels can also be passed along:
///
/// ```rust
/// use metrics::value;
///
/// # fn process() -> u64 { 42 }
/// fn handle_request() {
///     let rows_read = process();
///     value!("client.process_num_rows", rows_read, "resource" => "shard1", "table" => "posts");
/// }
/// # fn main() {}
/// ```
#[macro_export]
macro_rules! value {
    ($name:expr, $value:expr) => {
        if let Some(recorder) = $crate::try_recorder() {
            $crate::__private_api_record_histogram(recorder, $crate::Key::from_name($name), $value);
        }
    };

    ($name:expr, $value:expr, $($labels:tt)*) => {
        if let Some(recorder) = $crate::try_recorder() {
            let labels = $crate::labels!( $($labels)* );
            let key = $crate::Key::from_name_and_labels($name, labels);
            $crate::__private_api_record_histogram(recorder, key, $value);
        }
    };
}
