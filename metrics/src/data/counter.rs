use fnv::FnvBuildHasher;
use hashbrown::HashMap;
use crate::data::ScopedKey;

pub(crate) struct Counter {
    data: HashMap<ScopedKey, u64, FnvBuildHasher>,
}

impl Counter {
    pub fn new() -> Counter {
        Counter {
            data: HashMap::<ScopedKey, u64, FnvBuildHasher>::default(),
        }
    }

    pub fn update(&mut self, key: ScopedKey, delta: u64) {
        let value = self.data.entry(key).or_insert(0);
        *value = value.wrapping_add(delta);
    }

    pub fn values(&self) -> Vec<(ScopedKey, u64)> { self.data.iter().map(|(k, v)| (k.clone(), *v)).collect() }
}

#[cfg(test)]
mod tests {
    use super::{Counter, ScopedKey};

    #[test]
    fn test_counter_simple_update() {
        let mut counter = Counter::new();

        let key = ScopedKey(0, "foo".into());
        counter.update(key, 42);

        let key2 = ScopedKey(0, "foo".to_owned().into());
        counter.update(key2, 31);

        let values = counter.values();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].1, 73);
    }
}
