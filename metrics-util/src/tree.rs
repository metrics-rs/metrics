use serde::ser::{Serialize, Serializer};
use std::collections::HashMap;

/// An integer metric value.
pub enum Integer {
    /// A signed value.
    Signed(i64),

    /// An unsigned value.
    Unsigned(u64),
}

impl From<i64> for Integer {
    fn from(i: i64) -> Integer {
        Integer::Signed(i)
    }
}

impl From<u64> for Integer {
    fn from(i: u64) -> Integer {
        Integer::Unsigned(i)
    }
}

enum TreeEntry {
    Value(Integer),
    Nested(MetricsTree),
}

impl Serialize for TreeEntry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            TreeEntry::Value(value) => match value {
                Integer::Signed(i) => serializer.serialize_i64(*i),
                Integer::Unsigned(i) => serializer.serialize_u64(*i),
            },
            TreeEntry::Nested(tree) => tree.serialize(serializer),
        }
    }
}

/// A tree-structured metrics container.
///
/// Used for building a tree structure out of scoped metrics, where each level in the tree
/// represents a nested scope.
#[derive(Default)]
pub struct MetricsTree {
    contents: HashMap<String, TreeEntry>,
}

impl MetricsTree {
    /// Inserts a single value into the tree.
    pub fn insert_value<V: Into<Integer>>(
        &mut self,
        mut levels: Vec<String>,
        key: String,
        value: V,
    ) {
        match levels.len() {
            0 => {
                self.contents.insert(key, TreeEntry::Value(value.into()));
            }
            _ => {
                let name = levels.remove(0);
                let inner = self
                    .contents
                    .entry(name)
                    .or_insert_with(|| TreeEntry::Nested(MetricsTree::default()));

                if let TreeEntry::Nested(tree) = inner {
                    tree.insert_value(levels, key, value);
                }
            }
        }
    }

    /// Inserts multiple values into the tree.
    pub fn insert_values<V: Into<Integer>>(
        &mut self,
        mut levels: Vec<String>,
        values: Vec<(String, V)>,
    ) {
        match levels.len() {
            0 => {
                for v in values.into_iter() {
                    self.contents.insert(v.0, TreeEntry::Value(v.1.into()));
                }
            }
            _ => {
                let name = levels.remove(0);
                let inner = self
                    .contents
                    .entry(name)
                    .or_insert_with(|| TreeEntry::Nested(MetricsTree::default()));

                if let TreeEntry::Nested(tree) = inner {
                    tree.insert_values(levels, values);
                }
            }
        }
    }

    /// Clears all entries in the tree.
    pub fn clear(&mut self) {
        self.contents.clear();
    }
}

impl Serialize for MetricsTree {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sorted = self.contents.iter().collect::<Vec<_>>();
        sorted.sort_by_key(|p| p.0);

        serializer.collect_map(sorted)
    }
}
