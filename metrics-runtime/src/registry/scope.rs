use crate::common::{Scope, ScopeHandle};
use parking_lot::RwLock;
use std::collections::HashMap;

#[derive(Debug)]
struct Inner {
    id: u64,
    forward: HashMap<Scope, ScopeHandle>,
    backward: HashMap<ScopeHandle, Scope>,
}

impl Inner {
    pub fn new() -> Self {
        Inner {
            id: 1,
            forward: HashMap::new(),
            backward: HashMap::new(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ScopeRegistry {
    inner: RwLock<Inner>,
}

impl ScopeRegistry {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(Inner::new()),
        }
    }

    pub fn register(&self, scope: Scope) -> u64 {
        let mut wg = self.inner.write();

        // If the key is already registered, send back the existing scope ID.
        if wg.forward.contains_key(&scope) {
            return wg.forward.get(&scope).cloned().unwrap();
        }

        // Otherwise, take the current scope ID for this registration, store it, and increment
        // the scope ID counter for the next registration.
        let scope_id = wg.id;
        let _ = wg.forward.insert(scope.clone(), scope_id);
        let _ = wg.backward.insert(scope_id, scope);
        wg.id += 1;
        scope_id
    }

    pub fn get(&self, scope_id: ScopeHandle) -> Scope {
        // See if we have an entry for the scope ID, and clone the scope if so.
        let rg = self.inner.read();
        rg.backward.get(&scope_id).cloned().unwrap_or(Scope::Root)
    }
}

#[cfg(test)]
mod tests {
    use super::{Scope, ScopeRegistry};

    #[test]
    fn test_simple_write_then_read() {
        let nested1 = Scope::Root.add_part("nested1");
        let nested2 = nested1.clone().add_part("nested2");

        let sr = ScopeRegistry::new();

        let doesnt_exist0 = sr.get(0);
        let doesnt_exist1 = sr.get(1);
        let doesnt_exist2 = sr.get(2);

        assert_eq!(doesnt_exist0, Scope::Root);
        assert_eq!(doesnt_exist1, Scope::Root);
        assert_eq!(doesnt_exist2, Scope::Root);

        let nested1_original = nested1.clone();
        let nested1_id = sr.register(nested1);

        let nested2_original = nested2.clone();
        let nested2_id = sr.register(nested2);

        let exists1 = sr.get(nested1_id);
        let exists2 = sr.get(nested2_id);

        assert_eq!(exists1, nested1_original);
        assert_eq!(exists2, nested2_original);
    }
}
