#![allow(missing_docs)]

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Level(LevelInner);

impl Level {
    pub const TRACE: Self = Self(LevelInner::Trace);
    pub const DEBUG: Self = Self(LevelInner::Debug);
    pub const ERROR: Self = Self(LevelInner::Error);
    pub const WARN: Self = Self(LevelInner::Warn);
    pub const INFO: Self = Self(LevelInner::Info);
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LevelInner {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata<'a> {
    target: &'a str,
    level: Level,
    module_path: Option<&'a str>,
    file: Option<&'a str>,
    line: Option<u32>,
}

impl<'a> Metadata<'a> {
    pub const fn new(
        target: &'a str,
        level: Level,
        module_path: Option<&'a str>,
        file: Option<&'a str>,
        line: Option<u32>,
    ) -> Self {
        Self { target, level, module_path, file, line }
    }

    pub fn level(&self) -> &Level {
        &self.level
    }

    pub fn target(&self) -> &'a str {
        self.target
    }

    pub fn module_path(&self) -> Option<&'a str> {
        self.module_path
    }

    pub fn file(&self) -> Option<&'a str> {
        self.file
    }

    pub fn line(&self) -> Option<u32> {
        self.line
    }
}
