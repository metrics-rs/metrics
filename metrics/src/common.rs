use std::borrow::Cow;

/// An allocation-optimized string.
///
/// We specify `ScopedString` to attempt to get the best of both worlds: flexibility to provide a
/// static or dynamic (owned) string, while retaining the performance benefits of being able to
/// take ownership of owned strings and borrows of completely static strings.
pub type ScopedString = Cow<'static, str>;

/// Units for a given metric.
///
/// While metrics do not necessarily need to be tied to a particular unit to be recorded, some
/// downstream systems natively support defining units and so they can be specified during registration.
#[derive(Clone, Debug, PartialEq)]
pub enum Unit {
    // Dimensionless measurements.
    /// Count.
    Count,
    /// Percentage.
    Percent,
    // Time measurements.
    /// Seconds.
    Seconds,
    /// Milliseconds.
    Milliseconds,
    /// Microseconds.
    Microseconds,
    /// Nanoseconds.
    Nanoseconds,

    // Data measurement.
    /// Terabytes.
    Terabytes,
    /// Gigabytes.
    Gigabytes,
    /// Megabytes.
    Megabytes,
    /// Kilobytes.
    Kilobytes,
    /// Bytes.
    Bytes,
    /// Terabits.
    Terabits,
    /// Gigabits.
    Gigabits,
    /// Megabits.
    Megabits,
    /// Kilobits.
    Kilobits,
    /// Bits.
    Bits,

    // Rate measurements.
    /// Terabytes per second.
    TerabytesPerSecond,
    /// Gigabytes per second.
    GigabytesPerSecond,
    /// Megabytes per second.
    MegabytesPerSecond,
    /// Kilobytes per second.
    KilobytesPerSecond,
    /// Bytes per second.
    BytesPerSecond,
    /// Terabits per second.
    TerabitsPerSecond,
    /// Gigabits per second.
    GigabitsPerSecond,
    /// Megabits per second.
    MegabitsPerSecond,
    /// Kilobits per second.
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
            Unit::Terabytes => "terabytes",
            Unit::Gigabytes => "gigabytes",
            Unit::Megabytes => "megabytes",
            Unit::Kilobytes => "kilobytes",
            Unit::Bytes => "bytes",
            Unit::Terabits => "terabits",
            Unit::Gigabits => "gigabits",
            Unit::Megabits => "megabits",
            Unit::Kilobits => "kilobits",
            Unit::Bits => "bits",
            Unit::TerabytesPerSecond => "terabytes_per_second",
            Unit::GigabytesPerSecond => "gigabytes_per_second",
            Unit::MegabytesPerSecond => "megabytes_per_second",
            Unit::KilobytesPerSecond => "kilobytes_per_second",
            Unit::BytesPerSecond => "bytes_per_second",
            Unit::TerabitsPerSecond => "terabits_per_second",
            Unit::GigabitsPerSecond => "gigabits_per_second",
            Unit::MegabitsPerSecond => "megabits_per_second",
            Unit::KilobitsPerSecond => "kilobits_per_second",
            Unit::BitsPerSecond => "bits_per_second",
            Unit::CountPerSecond => "count_per_second",
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

impl IntoU64 for std::time::Duration {
    fn into_u64(self) -> u64 {
        self.as_nanos() as u64
    }
}

/// Helper method to allow monomorphization of values passed to the `histogram!` macro.
#[doc(hidden)]
pub fn __into_u64<V: IntoU64>(value: V) -> u64 {
    value.into_u64()
}
