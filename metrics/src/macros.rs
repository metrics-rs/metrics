#[doc(hidden)]
#[macro_export]
macro_rules! metadata_var {
    ($target:expr, $level:expr) => {{
        static METADATA: $crate::Metadata<'static> = $crate::Metadata::new(
            $target,
            $level,
            ::core::option::Option::Some(::core::module_path!()),
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
/// # Usage
///
/// `[opt: value,] <$name,> [$labels,]`
///
/// Only `$name` is required to initialize metrics.
///
/// All `opt`s MUST be specified before `$name` while `$labels` parameter block always go after `$name`
///
/// Following is brief explanation of parameters
///
/// ## Required parameters
///
/// - `$name` - Name of the metric. Can be expression that results in `String` or `&'static str`
///
/// ## Optional Parameters
///
/// Following parameters can be provided in any order
///
/// - `target:` - Specifies counter target. Defaults to `::core::module_path!()`.
/// - `level:` - Specifies counter level. Defaults to `INFO`.
/// - `describe:` - Specifies counter description to register for counter. If specified `$name` will be used twice.
/// - `unit:` - Specifies counter unit to register for counter if `describe:` is specified.
///
/// ## Labels
///
/// Labels can be passed as _one_ of following:
/// - Arbitrary number of `<key> => <value>` where `key` and `value` can be expression that results in `&'static str` or `String`
/// - Static reference to collection of **Label**
/// - Collection/iterator that implements [IntoLabels](trait.IntoLabels.html)
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
/// let gauge = counter!(format!("{}_via_format", "name"));
///
/// //Full counter customization example
/// let counter = counter!(
///     describe: "super counter",
///     unit: metrics::Unit::Bytes,
///     target: ::core::module_path!(),
///     level: metrics::Level::INFO,
///     "super_counter",
///     "label1" => "value1",
///     "label2" => "value2"
/// );
/// # }
/// ```
#[macro_export]
macro_rules! counter {
    ($($input:tt)*) => {
        $crate::__register_metric!(
            describe_counter,
            register_counter,
            describe = __internal_metric_description_none__,
            unit = __internal_metric_unit_none__,
            target = ::core::module_path!(),
            level = $crate::Level::INFO;
            $($input)*
        )
    };
}

#[doc(hidden)]
#[macro_export]
///Internal macro to register metric description when provided by metric creation macro
macro_rules! __describe_metric {
    // Do nothing if metric description is not set
    ($method:ident, __internal_metric_description_none__, __internal_metric_unit_none__, $($rest:tt)*) => {{}};
    // Show compilation error if `unit` only specified
    ($method:ident, __internal_metric_description_none__, $unit:expr, $($rest:tt)*) => {{
        compile_error!("'unit:' requires to specify parameter 'describe:'");
    }};
    // Found description only
    ($method:ident, $description:expr, __internal_metric_unit_none__, $name:expr) => {{
        $crate::with_recorder(|recorder| {
            recorder.$method(
                ::core::convert::Into::into($name),
                ::core::option::Option::None,
                ::core::convert::Into::into($description),
            );
        });
    }};
    // Found description + unit
    ($method:ident, $description:expr, $unit:expr, $name:expr) => {{
        $crate::with_recorder(|recorder| {
            recorder.$method(
                ::core::convert::Into::into($name),
                ::core::option::Option::Some($unit),
                ::core::convert::Into::into($description),
            );
        });
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __register_metric {
    // `target:` — replace the accumulator's `target` slot.
    (
        $describe:ident,
        $register:ident,
        describe = $description:tt,
        unit = $unit:tt,
        target = $_old:expr,
        level = $level:expr;
        target: $target:expr,
        $($rest:tt)*
    ) => {
        $crate::__register_metric!(
            $describe,
            $register,
            describe = $description,
            unit = $unit,
            target = $target,
            level = $level;
            $($rest)*
        )
    };
    // `level:` — replace the accumulator's `level` slot.
    (
        $describe:ident,
        $register:ident,
        describe = $description:tt,
        unit = $unit:tt,
        target = $target:expr,
        level = $_old:expr;
        level: $level:expr,
        $($rest:tt)*
    ) => {
        $crate::__register_metric!(
            $describe,
            $register,
            describe = $description,
            unit = $unit,
            target = $target,
            level = $level;
            $($rest)*
        )
    };
    // `describe:` — replace the accumulator's `describe` slot.
    (
        $describe:ident,
        $register:ident,
        describe = $old:tt,
        unit = $unit:tt,
        target = $target:expr,
        level = $level:expr;
        describe: $description:expr,
        $($rest:tt)*
    ) => {
        $crate::__register_metric!(
            $describe,
            $register,
            describe = $description,
            unit = $unit,
            target = $target,
            level = $level;
            $($rest)*
        )
    };
    // `unit:` — replace the accumulator's `unit` slot.
    (
        $describe:ident,
        $register:ident,
        describe = $description:tt,
        unit = $old:tt,
        target = $target:expr,
        level = $level:expr;
        unit: $unit:expr,
        $($rest:tt)*
    ) => {
        $crate::__register_metric!(
            $describe,
            $register,
            describe = $description,
            unit = $unit,
            target = $target,
            level = $level;
            $($rest)*
        )
    };
    // Terminator — emit the registration call.
    (
        $describe:ident,
        $register:ident,
        describe = $description:tt,
        unit = $unit:tt,
        target = $target:expr,
        level = $level:expr;
        $name:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?
    ) => {{
        $crate::__describe_metric!(describe_counter, $description, $unit, $name);

        let metric_key = $crate::key_var!($name $(, $label_key $(=> $label_value)?)*);
        let metadata = $crate::metadata_var!($target, $level);

        $crate::with_recorder(|recorder| recorder.$register(&metric_key, metadata))
    }};
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
/// # Usage
///
/// `[opt: value,] <$name,> [$labels,]`
///
/// Only `$name` is required to initialize metrics.
///
/// All `opt`s MUST be specified before `$name` while `$labels` parameter block always go after `$name`
///
/// Following is brief explanation of parameters
///
/// ## Required parameters
///
/// - `$name` - Name of the metric. Can be expression that results in `String` or `&'static str`
///
/// ## Optional Parameters
///
/// Following parameters can be provided in any order
///
/// - `target:` - Specifies counter target. Defaults to `::core::module_path!()`.
/// - `level:` - Specifies counter level. Defaults to `INFO`.
/// - `describe:` - Specifies counter description to register for counter. If specified `$name` will be used twice.
/// - `unit:` - Specifies counter unit to register for counter if `describe:` is specified.
///
/// ## Labels
///
/// Labels can be passed as _one_ of following:
/// - Arbitrary number of `<key> => <value>` where `key` and `value` can be expression that results in `&'static str` or `String`
/// - Static reference to collection of **Label**
/// - Collection/iterator that implements [IntoLabels](trait.IntoLabels.html)
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
/// //Full gauge customization example
/// let gauge = gauge!(
///     describe: "super gauge",
///     unit: metrics::Unit::Bytes,
///     target: ::core::module_path!(),
///     level: metrics::Level::INFO,
///     "super_gauge",
///     "label1" => "value1",
///     "label2" => "value2"
/// );
/// # }
/// ```
#[macro_export]
macro_rules! gauge {
    ($($input:tt)*) => {
        $crate::__register_metric!(
            describe_gauge,
            register_gauge,
            describe = __internal_metric_description_none__,
            unit = __internal_metric_unit_none__,
            target = ::core::module_path!(),
            level = $crate::Level::INFO;
            $($input)*
        )
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
/// # Usage
///
/// `[opt: value,] <$name,> [$labels,]`
///
/// Only `$name` is required to initialize metrics.
///
/// All `opt`s MUST be specified before `$name` while `$labels` parameter block always go after `$name`
///
/// Following is brief explanation of parameters
///
/// ## Required parameters
///
/// - `$name` - Name of the metric. Can be expression that results in `String` or `&'static str`
///
/// ## Optional Parameters
///
/// Following parameters can be provided in any order
///
/// - `target:` - Specifies counter target. Defaults to `::core::module_path!()`.
/// - `level:` - Specifies counter level. Defaults to `INFO`.
/// - `describe:` - Specifies counter description to register for counter. If specified `$name` will be used twice.
/// - `unit:` - Specifies counter unit to register for counter if `describe:` is specified.
///
/// ## Labels
///
/// Labels can be passed as _one_ of following:
/// - Arbitrary number of `<key> => <value>` where `key` and `value` can be expression that results in `&'static str` or `String`
/// - Static reference to collection of **Label**
/// - Collection/iterator that implements [IntoLabels](trait.IntoLabels.html)
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
/// //Full histogram customization example
/// let histogram = histogram!(
///     describe: "super counter",
///     unit: metrics::Unit::Bytes,
///     target: ::core::module_path!(),
///     level: metrics::Level::INFO,
///     "super_counter",
///     "label1" => "value1",
///     "label2" => "value2"
/// );
/// # }
/// ```
#[macro_export]
macro_rules! histogram {
    ($($input:tt)*) => {
        $crate::__register_metric!(
            describe_histogram,
            register_histogram,
            describe = __internal_metric_description_none__,
            unit = __internal_metric_unit_none__,
            target = ::core::module_path!(),
            level = $crate::Level::INFO;
            $($input)*
        )
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! describe {
    ($method:ident, $name:expr, $unit:expr, $description:expr $(,)?) => {{
        $crate::with_recorder(|recorder| {
            recorder.$method(
                ::core::convert::Into::into($name),
                ::core::option::Option::Some($unit),
                ::core::convert::Into::into($description),
            );
        });
    }};
    ($method:ident, $name:expr, $description:expr $(,)?) => {{
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
    ($name:expr, $unit:expr, $description:expr $(,)?) => {
        $crate::describe!(describe_counter, $name, $unit, $description)
    };
    ($name:expr, $description:expr $(,)?) => {
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
    ($name:expr, $unit:expr, $description:expr $(,)?) => {
        $crate::describe!(describe_gauge, $name, $unit, $description)
    };
    ($name:expr, $description:expr $(,)?) => {
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
    ($name:expr, $unit:expr, $description:expr $(,)?) => {
        $crate::describe!(describe_histogram, $name, $unit, $description)
    };
    ($name:expr, $description:expr $(,)?) => {
        $crate::describe!(describe_histogram, $name, $description)
    };
}
