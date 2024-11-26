/// Describes the level of verbosity of a metric event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Level(LevelInner);

impl Level {
    /// The "trace" level.
    pub const TRACE: Self = Self(LevelInner::Trace);

    /// The "debug" level.
    pub const DEBUG: Self = Self(LevelInner::Debug);

    /// The "info" level.
    pub const INFO: Self = Self(LevelInner::Info);

    /// The "warn" level.
    pub const WARN: Self = Self(LevelInner::Warn);

    /// The "error" level.
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

/// Metadata describing a metric.
///
/// All metrics have the following metadata:
///
/// - A [`target`](Metadata::target), a string that categorizes part of the system where metric originates from. The
///   `metrics`` macros default to using the module path where the metric originate as the target, but it may be
///   overridden.
/// - A [`level`](Metadata::level), specifying the verbosity the metric is emitted at.
///
/// In addition, the following optional metadata describing the source code location where the metric originated from
/// may be provided:
///
/// - The [module path](Metadata::module_path) of the source code location where the metric event originated.
///
/// Metadata usage is exporter-specific, and may be ignored entirely. See the documentation of the specific exporter
/// being used for more information.
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

    /// Returns the verbosity level of the metric.
    pub fn level(&self) -> &Level {
        &self.level
    }

    /// Returns the target of the metric.
    ///
    /// This specifies the part of the system where the metric originates from. Typically, this is the module path where
    /// the metric originated from, but can be overridden when registering a metric.
    pub fn target(&self) -> &'a str {
        self.target
    }

    /// Returns the module path of the metric.
    ///
    /// This specifies the module where the metric originates from, or `None` if the module path is unknown.
    pub fn module_path(&self) -> Option<&'a str> {
        self.module_path
    }
}
