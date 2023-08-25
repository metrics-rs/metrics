#[doc(hidden)]
#[macro_export]
macro_rules! metadata_var {
    ($target:expr, $level:expr) => {{
        static METADATA: ::metrics::Metadata<'static> =
            ::metrics::Metadata::new($target, $level, Some(module_path!()));
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
        1usize + $crate::count! { $($tail)* }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! key_var {
    ($name: literal) => {
        static METRIC_KEY: ::metrics::Key = ::metrics::Key::from_static_name($name);
        &METRIC_KEY
    };
    ($name:expr) => {
        ::metrics::Key::from_static_name(&$name)
    };
    ($name:literal, $($label_key:literal => $label_value:literal),*) => {{
        static LABELS: [::metrics::Label; $crate::count! { $($label_key)* }] = [
            $(::metrics::Label::from_static_parts(&$label_key, &$label_value)),*
        ];
        static METRIC_KEY: ::metrics::Key = ::metrics::Key::from_static_parts($name, &LABELS);
        &METRIC_KEY
    }};
    ($name:expr, $($label_key:literal => $label_value:literal),*) => {{
        static LABELS: [::metrics::Label; $crate::count! { $($label_key)* }] = [
            $(::metrics::Label::from_static_parts($label_key, $label_value)),*
        ];
        let metric_key = ::metrics::Key::from_static_parts(&$name, &LABELS);
        metric_key
    }};
    ($name:expr, $($label_key:expr => $label_value:expr),*) => {{
        let labels: [::metrics::Label; $crate::count! { $($label_key)* }] = [
            $(::metrics::Label::from_static_parts($label_key, $label_value)),*
        ];
        ::metrics::Key::from_static_parts(&$name, &labels)
    }};
    ($name:expr, $labels:expr) => {
        ::metrics::Key::from_parts(&$name, $labels)
    }
}

/// TODO
#[macro_export]
macro_rules! register_counter {
    ($(target: $target:expr,)? $(level: $level:expr,)? $name:expr $(, $label_key:expr => $label_value:expr)* $(,)?) => {
        {
            let metric_key = $crate::key_var! { $name,  $($label_key => $label_value),*  };
            let metadata = $crate::metadata_var! {
                $crate::default_target! { $($target)? },
                $crate::default_level! { $($level)? }
            };

            ::metrics::recorder().register_counter(&metric_key, metadata)
        }
    };
    ($(target: $target:expr,)? $(level: $level:expr,)? $name:expr $(, $labels:expr)? $(,)?) => {
        {
            let metric_key = $crate::key_var! { $name, $($labels)? };
            let metadata = $crate::metadata_var! {
                $crate::default_target! { $($target)? },
                $crate::default_level! { $($level)? }
            };

            ::metrics::recorder().register_counter(&metric_key, metadata)
        }
    };
}
