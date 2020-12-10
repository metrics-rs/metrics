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
    pub fn from_str(s: &str) -> Option<Unit> {
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
        match self {
            Unit::Seconds | Unit::Milliseconds | Unit::Microseconds | Unit::Nanoseconds => true,
            _ => false,
        }
    }

    /// Whether or not this unit relates to the measurement of data.
    pub fn is_data_based(&self) -> bool {
        match self {
            Unit::Tebibytes
            | Unit::Gigibytes
            | Unit::Mebibytes
            | Unit::Kibibytes
            | Unit::Bytes
            | Unit::TerabitsPerSecond
            | Unit::GigabitsPerSecond
            | Unit::MegabitsPerSecond
            | Unit::KilobitsPerSecond
            | Unit::BitsPerSecond => true,
            _ => false,
        }
    }

    /// Whether or not this unit relates to the measurement of data rates.
    pub fn is_data_rate_based(&self) -> bool {
        match self {
            Unit::TerabitsPerSecond
            | Unit::GigabitsPerSecond
            | Unit::MegabitsPerSecond
            | Unit::KilobitsPerSecond
            | Unit::BitsPerSecond => true,
            _ => false,
        }
    }
}

/// An object which can be converted into a `u64` representation.
///
/// This trait provides a mechanism for existing types, which have a natural representation
/// as an unsigned 64-bit integer, to be transparently passed in when recording a histogram.
pub trait IntoU64 {
    /// Converts this object to its `u64` representation.
    fn into_u64(self) -> u64;
}

impl IntoU64 for u64 {
    fn into_u64(self) -> u64 {
        self
    }
}

impl IntoU64 for core::time::Duration {
    fn into_u64(self) -> u64 {
        self.as_nanos() as u64
    }
}

/// Helper method to allow monomorphization of values passed to the `histogram!` macro.
#[doc(hidden)]
pub fn __into_u64<V: IntoU64>(value: V) -> u64 {
    value.into_u64()
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
            let parsed = Unit::from_str(s);
            assert_eq!(Some(variant), parsed);
        }
    }
}
