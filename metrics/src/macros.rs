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
macro_rules! default_target {
    () => {
        module_path!()
    };
    ($target:expr) => {
        $target
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! default_level {
    () => {
        $crate::Level::INFO
    };
    ($level:expr) => {
        $level
    };
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
    ($(target: $target:expr,)? $(level: $level:expr,)? $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        {
            let metric_key = $crate::key_var!($name, $($label_key $(=> $label_value)?),*);
            let metadata = $crate::metadata_var!(
                $crate::default_target!($($target)?),
                $crate::default_level!($($level)?)
            );

            $crate::recorder().register_counter(&metric_key, metadata)
        }
    };
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
    ($name:expr, $unit:expr, $description:expr) => {{
        if let ::core::option::Option::Some(recorder) = $crate::try_recorder() {
            recorder.describe_counter(
                $name.into(),
                ::core::option::Option::Some($unit),
                $description.into(),
            )
        }
    }};
    ($name:expr, $description:expr) => {{
        if let Some(recorder) = $crate::try_recorder() {
            recorder.describe_counter(
                $name.into(),
                ::core::option::Option::None,
                $description.into(),
            )
        }
    }};
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
    ($(target: $target:expr,)? $(level: $level:expr,)? $name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {{
        let handle = $crate::register_counter!($(target: $target,)? $(level: $level,)? $name $(, $label_key $(=> $label_value)?)*);
        handle.increment($op_val);
    }};
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
    ($(target: $target:expr,)? $(level: $level:expr,)? $name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {{
        let handle = $crate::register_counter!($(target: $target,)? $(level: $level,)? $name $(, $label_key $(=> $label_value)?)*);
        handle.absolute($op_val);
    }};
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
    ($(target: $target:expr,)? $(level: $level:expr,)? $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {{
        $crate::counter!($(target: $target,)? $(level: $level,)? $name, 1 $(, $label_key $(=> $label_value)?)*);
    }};
}
