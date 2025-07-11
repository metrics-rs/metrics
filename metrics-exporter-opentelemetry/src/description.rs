use metrics::{KeyName, SharedString, Unit};
use metrics_util::MetricKind;

use std::collections::HashMap;
use std::sync::{PoisonError, RwLock};

#[derive(Clone)]
pub struct DescriptionEntry {
    unit: Option<Unit>,
    description: SharedString,
}

impl DescriptionEntry {
    pub fn unit(&self) -> Option<Unit> {
        self.unit
    }
    pub fn description(&self) -> SharedString {
        self.description.clone()
    }
}

#[derive(Default)]
pub struct DescriptionTable {
    table: RwLock<HashMap<(KeyName, MetricKind), DescriptionEntry>>,
}

impl DescriptionTable {
    pub fn add_describe(
        &self,
        key_name: KeyName,
        metric_kind: MetricKind,
        unit: Option<Unit>,
        description: SharedString,
    ) {
        let new_entry = DescriptionEntry { unit, description };
        self.table
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .entry((key_name, metric_kind))
            .and_modify(|e| {
                *e = new_entry.clone();
            })
            .or_insert(new_entry);
    }

    pub fn get_describe(
        &self,
        key_name: KeyName,
        metric_kind: MetricKind,
    ) -> Option<DescriptionEntry> {
        let table = self.table.read().unwrap_or_else(PoisonError::into_inner);
        table.get(&(key_name, metric_kind)).cloned()
    }
}
