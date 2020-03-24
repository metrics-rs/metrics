/// Helper macro for generating a set of labels.
///
/// While a `Label` can be generated manually, most users will tend towards the key => value format
/// commonly used for defining hashes/maps in many programming languages.  This macro allows users
/// to do the exact same thing in calls that depend on [`metrics::IntoLabels`].
///
/// # Examples
/// ```rust
/// # #[macro_use] extern crate metrics;
/// # use metrics::IntoLabels;
/// fn takes_labels<L: IntoLabels>(name: &str, labels: L) {
///     println!("name: {} labels: {:?}", name, labels.into_labels());
/// }
///
/// takes_labels("requests_processed", labels!("request_type" => "admin"));
/// ```
#[macro_export]
macro_rules! labels {
    (@ { $($out:expr),* $(,)* } $(,)*) => {
        std::vec![ $($out),* ]
    };

    (@ { } $k:expr => $v:expr, $($rest:tt)*) => {
        $crate::labels!(@ { $crate::Label::new($k, $v) } $($rest)*)
    };

    (@ { $($out:expr),+ } $k:expr => $v:expr, $($rest:tt)*) => {
        $crate::labels!(@ { $($out),+, $crate::Label::new($k, $v) } $($rest)*)
    };

    ($($args:tt)*) => {
        $crate::labels!(@ { } $($args)*, )
    };
}
