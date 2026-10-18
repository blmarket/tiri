use crate::session::sessions;
use crate::shared::session::session;
use crate::shared::session::SessionRef;
use std::ffi::CStr;

fn node(name: &CStr) -> SessionRef {
    let node = session::new();
    unsafe {
        (*node.get()).name = name.to_owned();
    }
    node
}

#[test]
fn saved_name_survives_removal_of_current_successor_and_entire_index() {
    unsafe {
        let first = node(c"a1");
        let second = node(c"a2");
        let last = node(c"b1");
        sessions::insert(first.clone());
        sessions::insert(second.clone());
        sessions::insert(last.clone());
        let name = (*first.get()).name.clone();
        assert_eq!(
            sessions::after((*first.get()).name.as_c_str())
                .as_ref()
                .map_or(std::ptr::null_mut(), |owner| owner.get()),
            &mut *second.get() as *mut _
        );

        sessions::remove(&first);
        sessions::remove(&second);
        drop(first);
        drop(second);
        assert_eq!(
            sessions::after(&name)
                .as_ref()
                .map_or(std::ptr::null_mut(), |owner| owner.get()),
            &mut *last.get() as *mut _
        );

        let last_name = (*last.get()).name.clone();
        sessions::remove(&last);
        drop(last);
        assert!(sessions::is_empty());
        assert!(sessions::after(&last_name).is_none());

        let replacement = node(c"z1");
        sessions::insert(replacement.clone());
        assert_eq!(
            sessions::after(&last_name)
                .as_ref()
                .map_or(std::ptr::null_mut(), |owner| owner.get()),
            &mut *replacement.get() as *mut _
        );
        sessions::remove(&replacement);
    }
}

#[test]
fn previous_session_observer_requires_registration_without_retaining_session() {
    use std::rc::Rc;
    let previous = node(c"previous");
    let observer = Rc::downgrade(&previous);
    assert!(sessions::resolve(&observer).is_none());
    sessions::insert(previous.clone());
    let resolved = sessions::resolve(&observer).expect("registered session");
    assert!(Rc::ptr_eq(&resolved, &previous));
    sessions::remove(&previous);
    assert!(sessions::resolve(&observer).is_none());
    let replacement = node(c"previous");
    sessions::insert(replacement.clone());
    assert!(sessions::resolve(&observer).is_none());
    drop(previous);
    assert!(observer.upgrade().is_some());
    drop(resolved);
    assert!(observer.upgrade().is_none());
    assert!(sessions::resolve(&observer).is_none());
    sessions::remove(&replacement);
}

#[test]
fn duplicate_insertion_returns_retained_identity_and_wrong_owner_cannot_remove_it() {
    use std::rc::Rc;
    let original = node(c"same-name");
    let duplicate = node(c"same-name");
    let observer = Rc::downgrade(&original);
    assert!(sessions::insert(original.clone()).is_none());
    let retained = sessions::insert(duplicate.clone()).unwrap();
    assert!(Rc::ptr_eq(&retained, &original));
    assert!(sessions::remove(&duplicate).is_none());
    let removed = sessions::remove(&original).unwrap();
    assert!(Rc::ptr_eq(&removed, &original));
    assert!(sessions::is_empty());
    drop(removed);
    drop(original);
    assert!(
        observer.upgrade().is_some(),
        "duplicate result retains the indexed session"
    );
    drop(retained);
    assert!(observer.upgrade().is_none());
}
