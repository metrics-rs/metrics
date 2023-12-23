#[doc(hidden)]
#[macro_export]
macro_rules! metadata_var {
    ($target:expr, $level:expr) => {{
        static METADATA: $crate::Metadata<'static> = $crate::Metadata::new(
            $target,
            $level,
            ::core::option::Option::Some(::std::module_path!()),
        );
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
            $($crate::Label::from_static_parts($label_key, $label_value)),*
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
        let labels = ::std::vec![
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
/// counters, [`Counter`](crate::Counter) is provided which can be incremented or set to an absolute value.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # #![no_implicit_prelude]
/// # use ::std::convert::From;
/// # use ::std::format;
/// # use ::std::string::String;
/// # use metrics::counter;
/// # fn main() {
/// // A basic counter:
/// let counter = counter!("some_metric_name");
/// counter.increment(1);
///
/// // Specifying labels inline, including using constants for either the key or value:
/// let counter = counter!("some_metric_name", "service" => "http");
/// counter.absolute(42);
///
/// const SERVICE_LABEL: &'static str = "service";
/// const SERVICE_HTTP: &'static str = "http";
/// let counter = counter!("some_metric_name", SERVICE_LABEL => SERVICE_HTTP);
/// counter.increment(123);
///
/// // We can also pass labels by giving a vector or slice of key/value pairs.  In this scenario,
/// // a unit or description can still be passed in their respective positions:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// let counter = counter!("some_metric_name", &labels);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// let counter = counter!(name);
///
/// let counter = counter!(format!("{}_via_format", "name"));
/// # }
/// ```
#[macro_export]
macro_rules! counter {
    (target: $target:expr, level: $level:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {{
        let metric_key = $crate::key_var!($name $(, $label_key $(=> $label_value)?)*);
        let metadata = $crate::metadata_var!($target, $level);

        $crate::with_recorder(|recorder| recorder.register_counter(&metric_key, metadata))
    }};
    (target: $target:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::counter!(target: $target, level: $crate::Level::INFO, $name $(, $label_key $(=> $label_value)?)*)
    };
    (level: $level:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::counter!(target: ::std::module_path!(), level: $level, $name $(, $label_key $(=> $label_value)?)*)
    };
    ($name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::counter!(target: ::std::module_path!(), level: $crate::Level::INFO, $name $(, $label_key $(=> $label_value)?)*)
    };
}

/// Registers a gauge.
///
/// Gauges represent a single value that can go up or down over time, and always starts out with an
/// initial value of zero.
///
/// Metrics can be registered, which provides a handle to directly update that metric.  For gauges,
/// [`Gauge`](crate::Gauge) is provided which can be incremented, decrement, or set to an absolute value.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # #![no_implicit_prelude]
/// # use ::std::string::String;
/// # use ::std::format;
/// # use ::std::convert::From;
/// # use metrics::gauge;
/// # fn main() {
/// // A basic gauge:
/// let gauge = gauge!("some_metric_name");
/// gauge.increment(1.0);
///
/// // Specifying labels inline, including using constants for either the key or value:
/// let gauge = gauge!("some_metric_name", "service" => "http");
/// gauge.decrement(42.0);
///
/// const SERVICE_LABEL: &'static str = "service";
/// const SERVICE_HTTP: &'static str = "http";
/// let gauge = gauge!("some_metric_name", SERVICE_LABEL => SERVICE_HTTP);
/// gauge.increment(3.14);
///
/// // We can also pass labels by giving a vector or slice of key/value pairs.  In this scenario,
/// // a unit or description can still be passed in their respective positions:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// let gauge = gauge!("some_metric_name", &labels);
/// gauge.set(1337.0);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// let gauge = gauge!(name);
///
/// let gauge = gauge!(format!("{}_via_format", "name"));
/// # }
/// ```
#[macro_export]
macro_rules! gauge {
    (target: $target:expr, level: $level:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {{
        let metric_key = $crate::key_var!($name $(, $label_key $(=> $label_value)?)*);
        let metadata = $crate::metadata_var!($target, $level);

        $crate::with_recorder(|recorder| recorder.register_gauge(&metric_key, metadata))
    }};
    (target: $target:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::gauge!(target: $target, level: $crate::Level::INFO, $name $(, $label_key $(=> $label_value)?)*)
    };
    (level: $level:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::gauge!(target: ::std::module_path!(), level: $level, $name $(, $label_key $(=> $label_value)?)*)
    };
    ($name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::gauge!(target: ::std::module_path!(), level: $crate::Level::INFO, $name $(, $label_key $(=> $label_value)?)*)
    };
}

/// Registers a histogram.
///
/// Histograms measure the distribution of values for a given set of measurements, and start with no
/// initial values.
///
/// Metrics can be registered, which provides a handle to directly update that metric.  For
/// histograms, [`Histogram`](crate::Histogram) is provided which can record values.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # #![no_implicit_prelude]
/// # use ::std::string::String;
/// # use ::std::format;
/// # use ::std::convert::From;
/// # use metrics::histogram;
/// # fn main() {
/// // A basic histogram:
/// let histogram = histogram!("some_metric_name");
/// histogram.record(1.0);
///
/// // Specifying labels inline, including using constants for either the key or value:
/// let histogram = histogram!("some_metric_name", "service" => "http");
///
/// const SERVICE_LABEL: &'static str = "service";
/// const SERVICE_HTTP: &'static str = "http";
/// let histogram = histogram!("some_metric_name", SERVICE_LABEL => SERVICE_HTTP);
///
/// // We can also pass labels by giving a vector or slice of key/value pairs.  In this scenario,
/// // a unit or description can still be passed in their respective positions:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// let histogram = histogram!("some_metric_name", &labels);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// let histogram = histogram!(name);
///
/// let histogram = histogram!(format!("{}_via_format", "name"));
/// # }
/// ```
#[macro_export]
macro_rules! histogram {
    (target: $target:expr, level: $level:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {{
        let metric_key = $crate::key_var!($name $(, $label_key $(=> $label_value)?)*);
        let metadata = $crate::metadata_var!($target, $level);

        $crate::with_recorder(|recorder| recorder.register_histogram(&metric_key, metadata))
    }};
    (target: $target:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::histogram!(target: $target, level: $crate::Level::INFO, $name $(, $label_key $(=> $label_value)?)*)
    };
    (level: $level:expr, $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::histogram!(target: ::std::module_path!(), level: $level, $name $(, $label_key $(=> $label_value)?)*)
    };
    ($name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {
        $crate::histogram!(target: ::std::module_path!(), level: $crate::Level::INFO, $name $(, $label_key $(=> $label_value)?)*)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! describe {
    ($method:ident, $name:expr, $unit:expr, $description:expr) => {{
        $crate::with_recorder(|recorder| {
            recorder.$method(
                ::core::convert::Into::into($name),
                ::core::option::Option::Some($unit),
                ::core::convert::Into::into($description),
            );
        });
    }};
    ($method:ident, $name:expr, $description:expr) => {{
        $crate::with_recorder(|recorder| {
            recorder.$method(
                ::core::convert::Into::into($name),
                ::core::option::Option::None,
                ::core::convert::Into::into($description),
            );
        });
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
/// # #![no_implicit_prelude]
/// # use ::std::convert::From;
/// # use ::std::format;
/// # use ::std::string::String;
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
/// # #![no_implicit_prelude]
/// # use ::std::convert::From;
/// # use ::std::format;
/// # use ::std::string::String;
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
/// # #![no_implicit_prelude]
/// # use ::std::convert::From;
/// # use ::std::format;
/// # use ::std::string::String;
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
