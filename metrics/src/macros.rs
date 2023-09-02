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

/// TODO
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

/// TODO
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

/// TODO
#[macro_export]
macro_rules! counter {
    ($(target: $target:expr,)? $(level: $level:expr,)? $name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {{
        let handle = $crate::register_counter!($(target: $target,)? $(level: $level,)? $name $(, $label_key $(=> $label_value)?)*);
        handle.increment($op_val);
    }};
}

/// TODO
#[macro_export]
macro_rules! absolute_counter {
    ($(target: $target:expr,)? $(level: $level:expr,)? $name:expr, $op_val:expr $(, $label_key:expr $(=> $label_value:expr)?)* $(,)?) => {{
        let handle = $crate::register_counter!($(target: $target,)? $(level: $level,)? $name $(, $label_key $(=> $label_value)?)*);
        handle.absolute($op_val);
    }};
}
