use crate::data::ScopedKey;
use fnv::FnvBuildHasher;
use hashbrown::HashMap;

pub(crate) struct Gauge {
    data: HashMap<ScopedKey, i64, FnvBuildHasher>,
}

impl Gauge {
    pub fn new() -> Gauge {
        Gauge {
            data: HashMap::default(),
        }
    }

    pub fn update(&mut self, key: ScopedKey, value: i64) {
        let ivalue = self.data.entry(key).or_insert(0);
        *ivalue = value;
    }

    pub fn values(&self) -> Vec<(ScopedKey, i64)> {
        self.data.iter().map(|(k, v)| (k.clone(), *v)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{Gauge, ScopedKey};

    #[test]
    fn test_gauge_simple_update() {
        let mut gauge = Gauge::new();

        let key = ScopedKey(0, "foo".into());
        gauge.update(key, 42);

        let values = gauge.values();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].1, 42);

        let key2 = ScopedKey(0, "foo".to_owned().into());
        gauge.update(key2, 43);

        let values = gauge.values();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].1, 43);
    }
}
