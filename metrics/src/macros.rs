/// Records a counter.
///
/// Functionally equivalent to calling [`Recorder::record_counter`].
///
/// ### Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate metrics;
/// fn do_thing() {
///     let count: u64 = 42;
///     counter!("do_thing", count);
/// }
/// # fn main() {}
/// ```
///
/// Labels can also be passed along:
///
/// ```rust
/// # #[macro_use]
/// # extern crate metrics;
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
        $crate::__private_api_record_count($crate::Key::from_name($name), $value);
    };

    ($name:expr, $value:expr, $($labels:tt)*) => {
        let labels = $crate::labels!( $($labels)* );
        let key = $crate::Key::from_name_and_labels($name, labels);
        $crate::__private_api_record_count(key, $value);
    };
}

/// Records a gauge.
///
/// Functionally equivalent to calling [`Recorder::record_gauge`].
///
/// ### Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate metrics;
/// fn update_current_value() {
///     let value: i64 = -131;
///     gauge!("current_value", value);
/// }
/// # fn main() {}
/// ```
///
/// Labels can also be passed along:
///
/// ```rust
/// # #[macro_use]
/// # extern crate metrics;
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
        $crate::__private_api_record_gauge($crate::Key::from_name($name), $value);
    };

    ($name:expr, $value:expr, $($labels:tt)*) => {
        let labels = $crate::labels!( $($labels)* );
        let key = $crate::Key::from_name_and_labels($name, labels);
        $crate::__private_api_record_gauge(key, $value);
    };
}

/// Records a timing.
///
/// Functionally equivalent to calling [`Recorder::record_histogram`].
///
/// ### Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate metrics;
/// # use std::time::Instant;
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
/// # #[macro_use]
/// # extern crate metrics;
/// # use std::time::Instant;
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
#[macro_export]
macro_rules! timing {
    ($name:expr, $value:expr) => {
        $crate::__private_api_record_histogram($crate::Key::from_name($name), $value);
    };

    ($name:expr, $start:expr, $end:expr) => {
        timing!($name, $end - $start)
    };

    ($name:expr, $start:expr, $end:expr, $($labels:tt)*) => {
        timing!($name, $end - $start, $($labels)*)
    };

    ($name:expr, $value:expr, $($labels:tt)*) => {
        let labels = $crate::labels!( $($labels)* );
        let key = $crate::Key::from_name_and_labels($name, labels);
        $crate::__private_api_record_histogram(key, $value);
    };
}

/// Records a value.
///
/// Functionally equivalent to calling [`Recorder::record_histogram`].
///
/// ### Examples
///
/// ```rust
/// # #[macro_use]
/// # extern crate metrics;
/// # use std::time::Instant;
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
/// # #[macro_use]
/// # extern crate metrics;
/// # use std::time::Instant;
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
        $crate::__private_api_record_histogram($crate::Key::from_name($name), $value);
    };

    ($name:expr, $value:expr, $($labels:tt)*) => {
        let labels = $crate::labels!( $($labels)* );
        let key = $crate::Key::from_name_and_labels($name, labels);
        $crate::__private_api_record_histogram(key, $value);
    };
}
