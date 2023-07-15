/// Describes the level of verbosity of a metric event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Level(LevelInner);

impl Level {
    /// The "error" level.
    pub const TRACE: Self = Self(LevelInner::Trace);
    /// The "warn" level.
    pub const DEBUG: Self = Self(LevelInner::Debug);
    /// The "info" level.
    pub const INFO: Self = Self(LevelInner::Info);
    /// The "debug" level.
    pub const WARN: Self = Self(LevelInner::Warn);
    /// The "trace" level.
    pub const ERROR: Self = Self(LevelInner::Error);
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LevelInner {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

/// Metadata describing a metric event. This provides additional context to [`Recorder`](crate::Recorder), allowing for
/// fine-grained filtering.
///
/// Contains the following:
///
/// - A [`target`](Metadata::target), specifying the part of the system where the metric event occurred. When
/// initialized via the [metrics macro], and left unspecified, this defaults to the module path the
/// macro was invoked from.
/// - A [`level`](Metadata::level), specifying the verbosity the metric event is emitted at.
/// - An optional [`module_path`](Metadata::module_path), specifying the the module path the metric event was emitted
/// from.
///
/// [metrics_macros]: https://docs.rs/metrics/latest/metrics/#macros
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata<'a> {
    target: &'a str,
    level: Level,
    module_path: Option<&'a str>,
}

impl<'a> Metadata<'a> {
    /// Constructs a new [`Metadata`].
    pub const fn new(target: &'a str, level: Level, module_path: Option<&'a str>) -> Self {
        Self { target, level, module_path }
    }

    /// Returns the verbosity level of the metric event.
    pub fn level(&self) -> &Level {
        &self.level
    }

    /// Returns the target of the metric event. This specifies the part of the system where the event occurred.
    pub fn target(&self) -> &'a str {
        self.target
    }

    /// Returns the module path of the metric event. This specifies the module where the event occurred.
    pub fn module_path(&self) -> Option<&'a str> {
        self.module_path
    }
}
