use std::sync::Arc;

use metrics::SharedString;

#[test]
fn retained_owned_descriptions_share_contents_across_clones() {
    let owned = SharedString::from(String::from("Example description"));
    let retained = owned.to_retained();
    let copies = [retained.clone(), retained.to_retained()];

    assert_eq!(retained, owned);
    assert_ne!(retained.as_ptr(), owned.as_ptr());
    for copy in &copies {
        assert_eq!(copy.as_ptr(), retained.as_ptr());
    }

    drop(owned);
    drop(retained);
    assert_eq!(copies[0].as_ref(), "Example description");
}

#[test]
fn retained_static_descriptions_reuse_static_storage() {
    static DESCRIPTION: &str = "Example description";
    let retained = SharedString::const_str(DESCRIPTION).to_retained();
    assert_eq!(retained.as_ptr(), DESCRIPTION.as_ptr());
    assert_eq!(retained.clone().as_ptr(), DESCRIPTION.as_ptr());
}

#[test]
fn retained_shared_descriptions_preserve_arc_lifetime() {
    let description: Arc<str> = Arc::from("Example description");
    let weak = Arc::downgrade(&description);
    let retained = SharedString::from(Arc::clone(&description)).to_retained();
    let copy = retained.clone();

    assert_eq!(retained.as_ptr(), description.as_ptr());
    assert_eq!(copy.as_ptr(), description.as_ptr());
    drop(description);
    drop(retained);
    assert!(weak.upgrade().is_some());
    drop(copy);
    assert!(weak.upgrade().is_none());
}
