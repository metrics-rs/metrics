use std::hash::Hasher;

use t1ha::T1haHasher;

use crate::cow::Cow;

/// An allocation-optimized string.
///
/// We specify `SharedString` to attempt to get the best of both worlds: flexibility to provide a
/// static or dynamic (owned) string, while retaining the performance benefits of being able to
/// take ownership of owned strings and borrows of completely static strings.
///
/// `SharedString` can be converted to from either `&'static str` or `String`, with a method,
/// `const_str`, from constructing `SharedString` from `&'static str` in a `const` fashion.
pub type SharedString = Cow<'static, str>;

/// Key-specific hashing algorithm.
///
/// Currently uses T1ha (Fast Positive Hash).
///
/// For any use-case within a `metrics`-owned or adjacent crate, where hashing of a key is required,
/// this is the hasher that will be used.
#[derive(Default)]
pub struct KeyHasher(T1haHasher);

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0.finish()
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0.write(bytes)
    }
}

/// Value of a gauge operation.
#[derive(Clone, Debug)]
pub enum GaugeValue {
    /// Sets the value of the gauge to this value.
    Absolute(f64),
    /// Increments the value of the gauge by this much.
    Increment(f64),
    /// Decrements the value of the gauge by this much.
    Decrement(f64),
}

impl GaugeValue {
    /// Updates an input value based on this gauge value.
    pub fn update_value(&self, input: f64) -> f64 {
        match self {
            GaugeValue::Absolute(val) => *val,
            GaugeValue::Increment(val) => input + val,
            GaugeValue::Decrement(val) => input - val,
        }
    }
}

/// Units for a given metric.
///
/// While metrics do not necessarily need to be tied to a particular unit to be recorded, some
/// downstream systems natively support defining units and so they can be specified during registration.
#[derive(Clone, Debug, PartialEq)]
pub enum Unit {
    /// Count.
    Count,
    /// Percentage.
    Percent,
    /// Seconds.
    ///
    /// One second is equal to 1000 milliseconds.
    Seconds,
    /// Milliseconds.
    ///
    /// One millisecond is equal to 1000 microseconds.
    Milliseconds,
    /// Microseconds.
    ///
    /// One microsecond is equal to 1000 nanoseconds.
    Microseconds,
    /// Nanoseconds.
    Nanoseconds,
    /// Tebibytes.
    ///
    /// One tebibyte is equal to 1024 gigibytes.
    Tebibytes,
    /// Gigibytes.
    ///
    /// One gigibyte is equal to 1024 mebibytes.
    Gigibytes,
    /// Mebibytes.
    ///
    /// One mebibyte is equal to 1024 kibibytes.
    Mebibytes,
    /// Kibibytes.
    ///
    /// One kibibyte is equal to 1024 bytes.
    Kibibytes,
    /// Bytes.
    Bytes,
    /// Terabits per second.
    ///
    /// One terabit is equal to 1000 gigabits.
    TerabitsPerSecond,
    /// Gigabits per second.
    ///
    /// One gigabit is equal to 1000 megabits.
    GigabitsPerSecond,
    /// Megabits per second.
    ///
    /// One megabit is equal to 1000 kilobits.
    MegabitsPerSecond,
    /// Kilobits per second.
    ///
    /// One kilobit is equal to 1000 bits.
    KilobitsPerSecond,
    /// Bits per second.
    BitsPerSecond,
    /// Count per second.
    CountPerSecond,
}

impl Unit {
    /// Gets the string form of this `Unit`.
    pub fn as_str(&self) -> &str {
        match self {
            Unit::Count => "count",
            Unit::Percent => "percent",
            Unit::Seconds => "seconds",
            Unit::Milliseconds => "milliseconds",
            Unit::Microseconds => "microseconds",
            Unit::Nanoseconds => "nanoseconds",
            Unit::Tebibytes => "tebibytes",
            Unit::Gigibytes => "gigibytes",
            Unit::Mebibytes => "mebibytes",
            Unit::Kibibytes => "kibibytes",
            Unit::Bytes => "bytes",
            Unit::TerabitsPerSecond => "terabits_per_second",
            Unit::GigabitsPerSecond => "gigabits_per_second",
            Unit::MegabitsPerSecond => "megabits_per_second",
            Unit::KilobitsPerSecond => "kilobits_per_second",
            Unit::BitsPerSecond => "bits_per_second",
            Unit::CountPerSecond => "count_per_second",
        }
    }

    /// Gets the canonical string label for the given unit.
    ///
    /// For example, the canonical label for `Seconds` would be `s`, while for `Nanoseconds`,
    /// it would be `ns`.
    ///
    /// Not all units have a meaningful display label and so some may be empty.
    pub fn as_canonical_label(&self) -> &str {
        match self {
            Unit::Count => "",
            Unit::Percent => "%",
            Unit::Seconds => "s",
            Unit::Milliseconds => "ms",
            Unit::Microseconds => "μs",
            Unit::Nanoseconds => "ns",
            Unit::Tebibytes => "TiB",
            Unit::Gigibytes => "GiB",
            Unit::Mebibytes => "MiB",
            Unit::Kibibytes => "KiB",
            Unit::Bytes => "B",
            Unit::TerabitsPerSecond => "Tbps",
            Unit::GigabitsPerSecond => "Gbps",
            Unit::MegabitsPerSecond => "Mbps",
            Unit::KilobitsPerSecond => "kbps",
            Unit::BitsPerSecond => "bps",
            Unit::CountPerSecond => "/s",
        }
    }

    /// Converts the string representation of a unit back into `Unit` if possible.
    ///
    /// The value passed here should match the output of [`Unit::as_str`].
    pub fn from_string(s: &str) -> Option<Unit> {
        match s {
            "count" => Some(Unit::Count),
            "percent" => Some(Unit::Percent),
            "seconds" => Some(Unit::Seconds),
            "milliseconds" => Some(Unit::Milliseconds),
            "microseconds" => Some(Unit::Microseconds),
            "nanoseconds" => Some(Unit::Nanoseconds),
            "tebibytes" => Some(Unit::Tebibytes),
            "gigibytes" => Some(Unit::Gigibytes),
            "mebibytes" => Some(Unit::Mebibytes),
            "kibibytes" => Some(Unit::Kibibytes),
            "bytes" => Some(Unit::Bytes),
            "terabits_per_second" => Some(Unit::TerabitsPerSecond),
            "gigabits_per_second" => Some(Unit::GigabitsPerSecond),
            "megabits_per_second" => Some(Unit::MegabitsPerSecond),
            "kilobits_per_second" => Some(Unit::KilobitsPerSecond),
            "bits_per_second" => Some(Unit::BitsPerSecond),
            "count_per_second" => Some(Unit::CountPerSecond),
            _ => None,
        }
    }

    /// Whether or not this unit relates to the measurement of time.
    pub fn is_time_based(&self) -> bool {
        matches!(
            self,
            Unit::Seconds | Unit::Milliseconds | Unit::Microseconds | Unit::Nanoseconds
        )
    }

    /// Whether or not this unit relates to the measurement of data.
    pub fn is_data_based(&self) -> bool {
        matches!(
            self,
            Unit::Tebibytes
                | Unit::Gigibytes
                | Unit::Mebibytes
                | Unit::Kibibytes
                | Unit::Bytes
                | Unit::TerabitsPerSecond
                | Unit::GigabitsPerSecond
                | Unit::MegabitsPerSecond
                | Unit::KilobitsPerSecond
                | Unit::BitsPerSecond
        )
    }

    /// Whether or not this unit relates to the measurement of data rates.
    pub fn is_data_rate_based(&self) -> bool {
        matches!(
            self,
            Unit::TerabitsPerSecond
                | Unit::GigabitsPerSecond
                | Unit::MegabitsPerSecond
                | Unit::KilobitsPerSecond
                | Unit::BitsPerSecond
        )
    }
}

/// An object which can be converted into a `f64` representation.
///
/// This trait provides a mechanism for existing types, which have a natural representation
/// as a 64-bit floating-point number, to be transparently passed in when recording a histogram.
pub trait IntoF64 {
    /// Converts this object to its `f64` representation.
    fn into_f64(self) -> f64;
}

impl IntoF64 for f64 {
    fn into_f64(self) -> f64 {
        self
    }
}

impl IntoF64 for core::time::Duration {
    fn into_f64(self) -> f64 {
        self.as_secs_f64()
    }
}

/// Helper method to allow monomorphization of values passed to the `histogram!` macro.
#[doc(hidden)]
pub fn __into_f64<V: IntoF64>(value: V) -> f64 {
    value.into_f64()
}

#[cfg(test)]
mod tests {
    use super::Unit;

    #[test]
    fn test_unit_conversions() {
        let all_variants = vec![
            Unit::Count,
            Unit::Percent,
            Unit::Seconds,
            Unit::Milliseconds,
            Unit::Microseconds,
            Unit::Nanoseconds,
            Unit::Tebibytes,
            Unit::Gigibytes,
            Unit::Mebibytes,
            Unit::Kibibytes,
            Unit::Bytes,
            Unit::TerabitsPerSecond,
            Unit::GigabitsPerSecond,
            Unit::MegabitsPerSecond,
            Unit::KilobitsPerSecond,
            Unit::BitsPerSecond,
            Unit::CountPerSecond,
        ];

        for variant in all_variants {
            let s = variant.as_str();
            let parsed = Unit::from_string(s);
            assert_eq!(Some(variant), parsed);
        }
    }
}
