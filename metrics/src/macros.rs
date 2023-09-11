#[doc(hidden)]
#[macro_export]
macro_rules! metadata_var {
    ($target:expr, $level:expr) => {{
        static METADATA: $crate::Metadata<'static> =
            $crate::Metadata::new($target, $level, Some(module_path!()));
        &METADATA
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! count {
    () => {
        0usize
    };
    ($head:tt $($tail:tt)*) => {
        1usize + $crate::count!($($tail)*)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! key_var {
    ($name: literal) => {{
        static METRIC_KEY: $crate::Key = $crate::Key::from_static_name($name);
        &METRIC_KEY
    }};
    ($name:expr) => {
        $crate::Key::from_name($name)
    };
    ($name:literal, $($label_key:literal => $label_value:literal),*) => {{
        static LABELS: [$crate::Label; $crate::count!($($label_key)*)] = [
            $($crate::Label::from_static_parts(&$label_key, &$label_value)),*
        ];
        static METRIC_KEY: $crate::Key = $crate::Key::from_static_parts($name, &LABELS);
        &METRIC_KEY
    }};
    ($name:expr, $($label_key:literal => $label_value:literal),*) => {{
        static LABELS: [$crate::Label; $crate::count!($($label_key)*)] = [
            $($crate::Label::from_static_parts($label_key, $label_value)),*
        ];
        $crate::Key::from_static_labels($name, &LABELS)
    }};
    ($name:expr, $($label_key:expr => $label_value:expr),*) => {{
        let labels = vec![
            $($crate::Label::new($label_key, $label_value)),*
        ];
        $crate::Key::from_parts($name, labels)
    }};
    ($name:expr, $labels:expr) => {
        $crate::Key::from_parts($name, $labels)
    }
}

/// Registers a counter.
///
/// Counters represent a single monotonic value, which means the value can only be incremented, not
/// decremented, and always starts out with an initial value of zero.
///
/// Metrics can be registered, which provides a handle to directly update that metric.  For
/// counters, [`Counter`] is provided which can be incremented or set to an absolute value.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::register_counter;
/// # fn main() {
/// // A basic counter:
/// let counter = register_counter!("some_metric_name");
/// counter.increment(1);
///
/// // Specifying labels inline, including using constants for either the key or value:
/// let counter = register_counter!("some_metric_name", "service" => "http");
/// counter.absolute(42);
///
/// const SERVICE_LABEL: &'static str = "service";
/// const SERVICE_HTTP: &'static str = "http";
/// let counter = register_counter!("some_metric_name", SERVICE_LABEL => SERVICE_HTTP);
/// counter.increment(123);
///
/// // We can also pass labels by giving a vector or slice of key/value pairs.  In this scenario,
/// // a unit or description can still be passed in their respective positions:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// let counter = register_counter!("some_metric_name", &labels);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// let counter = register_counter!(name);
///
/// let counter = register_counter!(format!("{}_via_format", "name"));
/// # }
/// ```
#[macro_export]
macro_rules! register_counter {
    (target: $target:expr, level: $level:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {{
        let metric_key = $crate::key_var!($name $(, $label_key $(=> $label_value)?)*);
        let metadata = $crate::metadata_var!($target, $level);

        $crate::recorder().register_counter(&metric_key, metadata)
    }};
    (target: $target:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::register_counter!(target: $target, level: $crate::Level::INFO, $name $(, $label_key $(=> $label_value)?)*)
    };
    (level: $level:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::register_counter!(target: module_path!(), level: $level, $name $(, $label_key$(=> $label_value)?)*)
    };
    ($name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::register_counter!(target: module_path!(), level: $crate::Level::INFO, $name $(, $label_key$(=> $label_value)?)*)
    };
}

/// Registers a gauge.
///
/// Gauges represent a single value that can go up or down over time, and always starts out with an
/// initial value of zero.
///
/// Metrics can be registered, which provides a handle to directly update that metric.  For gauges,
/// [`Gauge`] is provided which can be incremented, decrement, or set to an absolute value.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::register_gauge;
/// # fn main() {
/// // A basic gauge:
/// let gauge = register_gauge!("some_metric_name");
/// gauge.increment(1.0);
///
/// // Specifying labels inline, including using constants for either the key or value:
/// let gauge = register_gauge!("some_metric_name", "service" => "http");
/// gauge.decrement(42.0);
///
/// const SERVICE_LABEL: &'static str = "service";
/// const SERVICE_HTTP: &'static str = "http";
/// let gauge = register_gauge!("some_metric_name", SERVICE_LABEL => SERVICE_HTTP);
/// gauge.increment(3.14);
///
/// // We can also pass labels by giving a vector or slice of key/value pairs.  In this scenario,
/// // a unit or description can still be passed in their respective positions:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// let gauge = register_gauge!("some_metric_name", &labels);
/// gauge.set(1337.0);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// let gauge = register_gauge!(name);
///
/// let gauge = register_gauge!(format!("{}_via_format", "name"));
/// # }
/// ```
#[macro_export]
macro_rules! register_gauge {
    (target: $target:expr, level: $level:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {{
        let metric_key = $crate::key_var!($name $(, $label_key $(=> $label_value)?)*);
        let metadata = $crate::metadata_var!($target, $level);

        $crate::recorder().register_gauge(&metric_key, metadata)
    }};
    (target: $target:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::register_gauge!(target: $target, level: $crate::Level::INFO, $name $(, $label_key $(=> $label_value)?)*)
    };
    (level: $level:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::register_gauge!(target: module_path!(), level: $level, $name $(, $label_key$(=> $label_value)?)*)
    };
    ($name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::register_gauge!(target: module_path!(), level: $crate::Level::INFO, $name $(, $label_key$(=> $label_value)?)*)
    };
}

/// Registers a histogram.
///
/// Histograms measure the distribution of values for a given set of measurements, and start with no
/// initial values.
///
/// Metrics can be registered, which provides a handle to directly update that metric.  For
/// histograms, [`Histogram`] is provided which can record values.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::register_histogram;
/// # fn main() {
/// // A basic histogram:
/// let histogram = register_histogram!("some_metric_name");
/// histogram.record(1.0);
///
/// // Specifying labels inline, including using constants for either the key or value:
/// let histogram = register_histogram!("some_metric_name", "service" => "http");
///
/// const SERVICE_LABEL: &'static str = "service";
/// const SERVICE_HTTP: &'static str = "http";
/// let histogram = register_histogram!("some_metric_name", SERVICE_LABEL => SERVICE_HTTP);
///
/// // We can also pass labels by giving a vector or slice of key/value pairs.  In this scenario,
/// // a unit or description can still be passed in their respective positions:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// let histogram = register_histogram!("some_metric_name", &labels);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// let histogram = register_histogram!(name);
///
/// let histogram = register_histogram!(format!("{}_via_format", "name"));
/// # }
/// ```
#[macro_export]
macro_rules! register_histogram {
    (target: $target:expr, level: $level:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {{
        let metric_key = $crate::key_var!($name $(, $label_key $(=> $label_value)?)*);
        let metadata = $crate::metadata_var!($target, $level);

        $crate::recorder().register_histogram(&metric_key, metadata)
    }};
    (target: $target:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::register_histogram!(target: $target, level: $crate::Level::INFO, $name $(, $label_key $(=> $label_value)?)*)
    };
    (level: $level:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::register_histogram!(target: module_path!(), level: $level, $name $(, $label_key$(=> $label_value)?)*)
    };
    ($name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::register_histogram!(target: module_path!(), level: $crate::Level::INFO, $name $(, $label_key$(=> $label_value)?)*)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! describe {
    ($method:ident, $name:expr, $unit:expr, $description:expr) => {{
        if let ::core::option::Option::Some(recorder) = $crate::try_recorder() {
            recorder.$method($name.into(), ::core::option::Option::Some($unit), $description.into())
        }
    }};
    ($method:ident, $name:expr, $description:expr) => {{
        if let Some(recorder) = $crate::try_recorder() {
            recorder.$method($name.into(), ::core::option::Option::None, $description.into())
        }
    }};
}

/// Describes a counter.
///
/// Counters represent a single monotonic value, which means the value can only be incremented, not
/// decremented, and always starts out with an initial value of zero.
///
/// Metrics can be described with a free-form string, and optionally, a unit can be provided to
/// describe the value and/or rate of the metric measurements.  Whether or not the installed
/// recorder does anything with the description, or optional unit, is implementation defined.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::describe_counter;
/// # use metrics::Unit;
/// # fn main() {
/// // A basic counter:
/// describe_counter!("some_metric_name", "my favorite counter");
///
/// // Providing a unit for a counter:
/// describe_counter!("some_metric_name", Unit::Bytes, "my favorite counter");
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// describe_counter!(name, "my favorite counter");
///
/// describe_counter!(format!("{}_via_format", "name"), "my favorite counter");
/// # }
/// ```
#[macro_export]
macro_rules! describe_counter {
    ($name:expr, $unit:expr, $description:expr) => {
        $crate::describe!(describe_counter, $name, $unit, $description)
    };
    ($name:expr, $description:expr) => {
        $crate::describe!(describe_counter, $name, $description)
    };
}

/// Describes a gauge.
///
/// Gauges represent a single value that can go up or down over time, and always starts out with an
/// initial value of zero.
///
/// Metrics can be described with a free-form string, and optionally, a unit can be provided to
/// describe the value and/or rate of the metric measurements.  Whether or not the installed
/// recorder does anything with the description, or optional unit, is implementation defined.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::describe_gauge;
/// # use metrics::Unit;
/// # fn main() {
/// // A basic gauge:
/// describe_gauge!("some_metric_name", "my favorite gauge");
///
/// // Providing a unit for a gauge:
/// describe_gauge!("some_metric_name", Unit::Bytes, "my favorite gauge");
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// describe_gauge!(name, "my favorite gauge");
///
/// describe_gauge!(format!("{}_via_format", "name"), "my favorite gauge");
/// # }
/// ```
#[macro_export]
macro_rules! describe_gauge {
    ($name:expr, $unit:expr, $description:expr) => {
        $crate::describe!(describe_gauge, $name, $unit, $description)
    };
    ($name:expr, $description:expr) => {
        $crate::describe!(describe_gauge, $name, $description)
    };
}

/// Describes a histogram.
///
/// Histograms measure the distribution of values for a given set of measurements, and start with no
/// initial values.
///
/// Metrics can be described with a free-form string, and optionally, a unit can be provided to
/// describe the value and/or rate of the metric measurements.  Whether or not the installed
/// recorder does anything with the description, or optional unit, is implementation defined.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::describe_histogram;
/// # use metrics::Unit;
/// # fn main() {
/// // A basic histogram:
/// describe_histogram!("some_metric_name", "my favorite histogram");
///
/// // Providing a unit for a histogram:
/// describe_histogram!("some_metric_name", Unit::Bytes, "my favorite histogram");
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// describe_histogram!(name, "my favorite histogram");
///
/// describe_histogram!(format!("{}_via_format", "name"), "my favorite histogram");
/// # }
/// ```
#[macro_export]
macro_rules! describe_histogram {
    ($name:expr, $unit:expr, $description:expr) => {
        $crate::describe!(describe_histogram, $name, $unit, $description)
    };
    ($name:expr, $description:expr) => {
        $crate::describe!(describe_histogram, $name, $description)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! method {
    ($register:ident, $method:ident, target: $target:expr, level: $level:expr, $name:expr, $op_val: expr $(, $label_key:expr $(=> $label_value:expr)?)*) => {
        let metric_key = $crate::key_var!($name $(, $label_key $(=> $label_value)?)*);
        let metadata = $crate::metadata_var!($target, $level);

        if let Some(recorder) = $crate::try_recorder() {
            let handle = recorder.$register(&metric_key, &metadata);
            handle.$method($op_val);
        }
    };
    ($register:ident, $method:ident, target: $target:expr, $name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)*) => {
        $crate::method!($register, $method, target: $target, level: $crate::Level::INFO, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
    ($register:ident, $method:ident, level: $level:expr, $name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)*) => {
        $crate::method!($register, $method, target: module_path!(), level: $level, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
    ($register:ident, $method:ident, $name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)*) => {
        $crate::method!($register, $method, target: module_path!(), level: $crate::Level::INFO, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
}

/// Increments a counter.
///
/// Counters represent a single monotonic value, which means the value can only be incremented, not
/// decremented, and always starts out with an initial value of zero.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::{counter, Level};
/// # fn main() {
/// // A basic counter:
/// counter!("some_metric_name", 12);
///
/// // A basic counter with level and target specified:
/// counter!(target: "specific_target", level: Level::DEBUG, "some_metric_name", 12);
///
/// // Specifying labels inline, including using constants for either the key or value:
/// counter!("some_metric_name", 12, "service" => "http");
///
/// const SERVICE_LABEL: &'static str = "service";
/// const SERVICE_HTTP: &'static str = "http";
/// counter!("some_metric_name", 12, SERVICE_LABEL => SERVICE_HTTP);
///
/// // We can also pass labels by giving a vector or slice of key/value pairs:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// counter!("some_metric_name", 12, &labels);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// counter!(name, 12);
///
/// counter!(format!("{}_via_format", "name"), 12);
/// # }
/// ```
#[macro_export]
macro_rules! counter {
    (target: $target:expr, level: $level:expr, $name:expr, $op_val: expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_counter, increment, target: $target, level: $level, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
    (target: $target:expr, $name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_counter, increment, target: $target, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
    (level: $level:expr, $name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_counter, increment, level: $level, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
    ($name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_counter, increment, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
}

/// Increments a gauge.
///
/// Gauges represent a single value that can go up or down over time, and always starts out with an
/// initial value of zero.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::{increment_gauge, Level};
/// # fn main() {
/// // A basic gauge:
/// increment_gauge!("some_metric_name", 42.2222);
///
/// // A basic gauge with level and target specified:
/// increment_gauge!(target: "specific_target", level: Level::DEBUG, "some_metric_name", 42.2222);
///
/// // Specifying labels inline, including using constants for either the key or value:
/// increment_gauge!("some_metric_name", 66.6666, "service" => "http");
///
/// const SERVICE_LABEL: &'static str = "service";
/// const SERVICE_HTTP: &'static str = "http";
/// increment_gauge!("some_metric_name", 66.6666, SERVICE_LABEL => SERVICE_HTTP);
///
/// // We can also pass labels by giving a vector or slice of key/value pairs:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// increment_gauge!("some_metric_name", 42.42, &labels);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// increment_gauge!(name, 800.85);
///
/// increment_gauge!(format!("{}_via_format", "name"), 3.14);
/// # }
/// ```
#[macro_export]
macro_rules! increment_gauge {
    (target: $target:expr, level: $level:expr, $name:expr, $op_val: expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_gauge, increment, target: $target, level: $level, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
    (target: $target:expr, $name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_gauge, increment, target: $target, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
    (level: $level:expr, $name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_gauge, increment, level: $level, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
    ($name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_gauge, increment, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
}

/// Decrements a gauge.
///
/// Gauges represent a single value that can go up or down over time, and always starts out with an
/// initial value of zero.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::{decrement_gauge, Level};
/// # fn main() {
/// // A basic gauge:
/// decrement_gauge!("some_metric_name", 42.2222);
///
/// // A basic gauge with level and target specified:
/// decrement_gauge!(target: "specific_target", level: Level::DEBUG, "some_metric_name", 42.2222);
///
/// // Specifying labels inline, including using constants for either the key or value:
/// decrement_gauge!("some_metric_name", 66.6666, "service" => "http");
///
/// const SERVICE_LABEL: &'static str = "service";
/// const SERVICE_HTTP: &'static str = "http";
/// decrement_gauge!("some_metric_name", 66.6666, SERVICE_LABEL => SERVICE_HTTP);
///
/// // We can also pass labels by giving a vector or slice of key/value pairs:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// decrement_gauge!("some_metric_name", 42.42, &labels);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// decrement_gauge!(name, 800.85);
///
/// decrement_gauge!(format!("{}_via_format", "name"), 3.14);
/// # }
/// ```
#[macro_export]
macro_rules! decrement_gauge {
    (target: $target:expr, level: $level:expr, $name:expr, $op_val: expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_gauge, decrement, target: $target, level: $level, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
    (target: $target:expr, $name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_gauge, decrement, target: $target, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
    (level: $level:expr, $name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_gauge, decrement, level: $level, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
    ($name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_gauge, decrement, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
}

/// Sets a counter to an absolute value.
///
/// Counters represent a single monotonic value, which means the value can only be incremented, not
/// decremented, and will always start out with an initial value of zero.
///
/// Using this macro, users can specify an absolute value for the counter instead of the typical
/// delta.  This can be useful when dealing with forwarding metrics from an external system into the
/// normal application metrics, without having to track the delta of the metrics from the external
/// system.  Users should beware, though, that implementations will enforce the monotonicity
/// property of counters by refusing to update the value unless it is greater than current value of
/// the counter.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::{absolute_counter, Level};
/// # fn main() {
/// // A basic counter:
/// absolute_counter!("some_metric_name", 12);
///
/// // A basic counter with level and target specified:
/// absolute_counter!(target: "specific_target", level: Level::DEBUG, "some_metric_name", 12);
///
/// // Specifying labels inline, including using constants for either the key or value:
/// absolute_counter!("some_metric_name", 13, "service" => "http");
///
/// const SERVICE_LABEL: &'static str = "service";
/// const SERVICE_HTTP: &'static str = "http";
/// absolute_counter!("some_metric_name", 13, SERVICE_LABEL => SERVICE_HTTP);
///
/// // We can also pass labels by giving a vector or slice of key/value pairs:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// absolute_counter!("some_metric_name", 14, &labels);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// absolute_counter!(name, 15);
///
/// absolute_counter!(format!("{}_via_format", "name"), 16);
/// # }
/// ```
#[macro_export]
macro_rules! absolute_counter {
    (target: $target:expr, level: $level:expr, $name:expr, $op_val: expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_counter, absolute, target: $target, level: $level, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
    (target: $target:expr, $name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_counter, absolute, target: $target, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
    (level: $level:expr, $name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_counter, absolute, level: $level, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
    ($name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_counter, absolute, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
}

/// Updates a gauge.
///
/// Gauges represent a single value that can go up or down over time, and always starts out with an
/// initial value of zero.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::{gauge, Level};
/// # fn main() {
/// // A basic gauge:
/// gauge!("some_metric_name", 42.2222);
///
/// // A basic gauge with level and target specified:
/// gauge!(target: "specific_target", level: Level::DEBUG, "some_metric_name", 42.2222);
///
/// // Specifying labels inline, including using constants for either the key or value:
/// gauge!("some_metric_name", 66.6666, "service" => "http");
///
/// const SERVICE_LABEL: &'static str = "service";
/// const SERVICE_HTTP: &'static str = "http";
/// gauge!("some_metric_name", 66.6666, SERVICE_LABEL => SERVICE_HTTP);
///
/// // We can also pass labels by giving a vector or slice of key/value pairs:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// gauge!("some_metric_name", 42.42, &labels);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// gauge!(name, 800.85);
///
/// gauge!(format!("{}_via_format", "name"), 3.14);
/// # }
/// ```
#[macro_export]
macro_rules! gauge {
    (target: $target:expr, level: $level:expr, $name:expr, $op_val: expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_gauge, set, target: $target, level: $level, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
    (target: $target:expr, $name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_gauge, set, target: $target, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
    (level: $level:expr, $name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_gauge, set, level: $level, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
    ($name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_gauge, set, $name, $op_val $(, $label_key $(=> $label_value)?)*)
    };
}

/// Records a histogram.
///
/// Histograms measure the distribution of values for a given set of measurements, and start with no
/// initial values.
///
/// # Implicit conversions
/// Histograms are represented as `f64` values, but often come from another source, such as a time
/// measurement.  By default, `histogram!` will accept a `f64` directly or a
/// [`Duration`](std::time::Duration), which uses the floating-point number of seconds represents by
/// the duration.
///
/// External libraries and applications can create their own conversions by implementing the
/// [`IntoF64`] trait for their types, which is required for the value being passed to `histogram!`.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::{histogram, Level};
/// # use std::time::Duration;
/// # fn main() {
/// // A basic histogram:
/// histogram!("some_metric_name", 34.3);
///
/// // A basic histogram with level and target specified:
/// histogram!(target: "specific_target", level: Level::DEBUG, "some_metric_name", 34.3);
///
/// // An implicit conversion from `Duration`:
/// let d = Duration::from_millis(17);
/// histogram!("some_metric_name", d);
///
/// // Specifying labels inline, including using constants for either the key or value:
/// histogram!("some_metric_name", 38.0, "service" => "http");
///
/// const SERVICE_LABEL: &'static str = "service";
/// const SERVICE_HTTP: &'static str = "http";
/// histogram!("some_metric_name", 38.0, SERVICE_LABEL => SERVICE_HTTP);
///
/// // We can also pass labels by giving a vector or slice of key/value pairs:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// histogram!("some_metric_name", 1337.5, &labels);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// histogram!(name, 800.85);
///
/// histogram!(format!("{}_via_format", "name"), 3.14);
/// # }
/// ```
#[macro_export]
macro_rules! histogram {
    (target: $target:expr, level: $level:expr, $name:expr, $op_val: expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_histogram, record, target: $target, level: $level, $name, $crate::__into_f64($op_val) $(, $label_key $(=> $label_value)?)*)
    };
    (target: $target:expr, $name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_histogram, record, target: $target, $name, $crate::__into_f64($op_val) $(, $label_key $(=> $label_value)?)*)
    };
    (level: $level:expr, $name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_histogram, record, level: $level, $name, $crate::__into_f64($op_val) $(, $label_key $(=> $label_value)?)*)
    };
    ($name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_histogram, record, $name, $crate::__into_f64($op_val) $(, $label_key $(=> $label_value)?)*)
    };
}

/// Increments a counter.
///
/// Counters represent a single monotonic value, which means the value can only be incremented, not
/// decremented, and always starts out with an initial value of zero.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::{counter, Level};
/// # fn main() {
/// // A basic counter:
/// counter!("some_metric_name", 12);
///
/// // A basic counter with level and target specified:
/// counter!(target: "specific_target", level: Level::DEBUG, "some_metric_name", 12);
///
/// // Specifying labels inline, including using constants for either the key or value:
/// counter!("some_metric_name", 12, "service" => "http");
///
/// const SERVICE_LABEL: &'static str = "service";
/// const SERVICE_HTTP: &'static str = "http";
/// counter!("some_metric_name", 12, SERVICE_LABEL => SERVICE_HTTP);
///
/// // We can also pass labels by giving a vector or slice of key/value pairs:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// counter!("some_metric_name", 12, &labels);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// counter!(name, 12);
///
/// counter!(format!("{}_via_format", "name"), 12);
/// # }
/// ```
#[macro_export]
macro_rules! increment_counter {
    (target: $target:expr, level: $level:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_counter, increment, target: $target, level: $level, $name, 1 $(, $label_key $(=> $label_value)?)*)
    };
    (target: $target:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_counter, increment, target: $target, $name, 1 $(, $label_key $(=> $label_value)?)*)
    };
    (level: $level:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_counter, increment, level: $level, $name, 1 $(, $label_key $(=> $label_value)?)*)
    };
    ($name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::method!(register_counter, increment, $name, 1 $(, $label_key $(=> $label_value)?)*)
    };
}
