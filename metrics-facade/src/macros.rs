#[macro_export]
macro_rules! counter {
    ($name:tt, $value:expr) => ({
        $crate::__private_api_record_count($name, $value);
    })
}

#[macro_export]
macro_rules! gauge {
    ($name:tt, $value:expr) => ({
        $crate::__private_api_record_gauge($name, $value);
    })
}

#[macro_export]
macro_rules! timing {
    ($name:tt, $start:expr, $end:expr) => ({
        let delta = $end - $start;
        $crate::__private_api_record_histogram($name, delta);
    });
    ($name:tt, $value:expr) => ({
        $crate::__private_api_record_histogram($name, $value);
    })
}

#[macro_export]
macro_rules! value {
    ($name:tt, $value:expr) => ({
        $crate::__private_api_record_histogram($name, $value);
    })
}
