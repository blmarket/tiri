use crate::src::session::*;
use crate::src::shared::session::SessionRef;
use crate::src::shared::session::{session, sessions};
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
        let mut head = sessions { storage: None };
        let first = node(c"a1");
        let second = node(c"a2");
        let last = node(c"b1");
        sessions_insert(&mut head, first.clone());
        sessions_insert(&mut head, second.clone());
        sessions_insert(&mut head, last.clone());
        let name = CStr::from_ptr(((*first.get()).name).as_ptr().cast_mut())
            .to_bytes()
            .to_vec();
        assert_eq!(
            sessions_next(&*first.get())
                .as_ref()
                .map_or(std::ptr::null_mut(), |owner| owner.get()),
            &mut *second.get() as *mut _
        );

        // Group destruction removes the current session AND its cached successor.
        sessions_remove(&mut head, &first);
        sessions_remove(&mut head, &second);
        drop(first);
        drop(second);
        assert_eq!(
            sessions_after(&head, &name)
                .as_ref()
                .map_or(std::ptr::null_mut(), |owner| owner.get()),
            &mut *last.get() as *mut _
        );

        let last_name = CStr::from_ptr(((*last.get()).name).as_ptr().cast_mut())
            .to_bytes()
            .to_vec();
        sessions_remove(&mut head, &last);
        drop(last);
        assert!(head.storage.is_none());
        assert!(sessions_after(&head, &last_name).is_none());

        // Resuming reads the head afresh even after its previous map was freed.
        let replacement = node(c"z1");
        sessions_insert(&mut head, replacement.clone());
        assert_eq!(
            sessions_after(&head, &last_name)
                .as_ref()
                .map_or(std::ptr::null_mut(), |owner| owner.get()),
            &mut *replacement.get() as *mut _
        );
        sessions_remove(&mut head, &replacement);
    }
}

#[test]
fn previous_session_observer_requires_registration_without_retaining_session() {
    use std::rc::Rc;
    unsafe {
        let mut head = sessions { storage: None };
        let previous = node(c"previous");
        let observer = Rc::downgrade(&previous);
        assert!(sessions_resolve(&head, &observer).is_none());
        sessions_insert(&mut head, previous.clone());
        let resolved = sessions_resolve(&head, &observer).expect("registered session");
        assert!(Rc::ptr_eq(&resolved, &previous));
        sessions_remove(&mut head, &previous);
        // Queued work may retain the removed session, but it is no longer a target.
        assert!(sessions_resolve(&head, &observer).is_none());
        let replacement = node(c"previous");
        sessions_insert(&mut head, replacement.clone());
        assert!(sessions_resolve(&head, &observer).is_none());
        drop(previous);
        assert!(observer.upgrade().is_some());
        drop(resolved);
        assert!(observer.upgrade().is_none());
        assert!(sessions_resolve(&head, &observer).is_none());
        sessions_remove(&mut head, &replacement);
    }
}

#[test]
fn duplicate_insertion_returns_retained_identity_and_wrong_owner_cannot_remove_it() {
    use std::rc::Rc;
    unsafe {
        let mut head = sessions { storage: None };
        let original = node(c"same-name");
        let duplicate = node(c"same-name");
        let observer = Rc::downgrade(&original);
        assert!(sessions_insert(&mut head, original.clone()).is_none());
        let retained = sessions_insert(&mut head, duplicate.clone()).unwrap();
        assert!(Rc::ptr_eq(&retained, &original));
        assert!(sessions_remove(&mut head, &duplicate).is_none());
        let removed = sessions_remove(&mut head, &original).unwrap();
        assert!(Rc::ptr_eq(&removed, &original));
        assert!(head.storage.is_none());
        drop(removed);
        drop(original);
        assert!(
            observer.upgrade().is_some(),
            "duplicate result retains the indexed session"
        );
        drop(retained);
        assert!(observer.upgrade().is_none());
    }
}
