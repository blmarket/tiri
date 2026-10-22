use crate::cmd::queue::cmdq_print;
use crate::format::bytes::nullable_cstr;
use crate::format::bytes::write_cstr;
use crate::server_client::Client as _;
use crate::server_client::ClientRegistry;
use crate::shared::abi::*;
use crate::shared::abi::{__gid_t, __uid_t, gid_t, id_t, uid_t};
use crate::shared::account::{group, passwd};
use crate::shared::client::ClientRef;
use crate::shared::client::CLIENT_READONLY;
use crate::shared::command::cmdq_item;
use crate::shared::server_acl::SERVER_ACL_IS_GROUP;
use libc::{getgrgid, getpwuid, getuid};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct server_acl_entry {
    pub id: id_t,
    pub flags: ::core::ffi::c_int,
}
#[derive(Default)]
pub struct server_acl_entries {
    pub(crate) entries: std::collections::BTreeMap<(bool, id_t), server_acl_entry>,
}
pub const SERVER_ACL_READONLY: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;

fn server_acl_key(id: id_t, flags: ::core::ffi::c_int) -> (bool, id_t) {
    (flags & SERVER_ACL_IS_GROUP != 0, id)
}

thread_local! {
    static SERVER_ACL_ENTRIES: std::cell::RefCell<server_acl_entries> = const {
        std::cell::RefCell::new(server_acl_entries {
            entries: std::collections::BTreeMap::new(),
        })
    };
}

/// Entries are plain values: each call borrows the list only for its own
/// duration and hands out copies, never references into the map.
impl server_acl_entries {
    pub fn find(id: id_t, flags: ::core::ffi::c_int) -> Option<server_acl_entry> {
        SERVER_ACL_ENTRIES.with_borrow(|acl| acl.entries.get(&server_acl_key(id, flags)).copied())
    }

    /// Adds the entry unless one with the same key exists; returns whether it
    /// was added.
    pub fn insert(entry: server_acl_entry) -> bool {
        SERVER_ACL_ENTRIES.with_borrow_mut(|acl| {
            match acl.entries.entry(server_acl_key(entry.id, entry.flags)) {
                std::collections::btree_map::Entry::Occupied(_) => false,
                std::collections::btree_map::Entry::Vacant(vacant) => {
                    vacant.insert(entry);
                    true
                }
            }
        })
    }

    pub fn remove(id: id_t, flags: ::core::ffi::c_int) -> bool {
        SERVER_ACL_ENTRIES
            .with_borrow_mut(|acl| acl.entries.remove(&server_acl_key(id, flags)).is_some())
    }

    /// Applies `op` to the matching entry; returns whether one exists.
    pub fn update(
        id: id_t,
        flags: ::core::ffi::c_int,
        op: impl FnOnce(&mut server_acl_entry),
    ) -> bool {
        SERVER_ACL_ENTRIES.with_borrow_mut(|acl| {
            acl.entries
                .get_mut(&server_acl_key(id, flags))
                .map(op)
                .is_some()
        })
    }

    /// Entries in display order: users before groups, then by id.
    pub fn snapshot() -> Vec<server_acl_entry> {
        SERVER_ACL_ENTRIES.with_borrow(|acl| acl.entries.values().copied().collect())
    }

    pub fn clear() {
        SERVER_ACL_ENTRIES.with_borrow_mut(|acl| acl.entries.clear());
    }
}

fn server_acl_check(c: &ClientRef) -> Option<server_acl_entry> {
    let uid: uid_t = c.peer_uid();
    if uid == -(1 as ::core::ffi::c_int) as uid_t {
        return None;
    }
    if let Some(entry) = server_acl_entries::find(uid as id_t, 0) {
        return Some(entry);
    }
    let gid: gid_t = c.peer_gid();
    if gid == -(1 as ::core::ffi::c_int) as gid_t {
        return None;
    }
    server_acl_entries::find(gid as id_t, SERVER_ACL_IS_GROUP)
}
fn server_acl_update() {
    let mut c: Option<ClientRef> = None;
    let mut registry_c_owner = ClientRegistry::first();
    c = registry_c_owner.clone();
    while !c.is_none() {
        match server_acl_check(c.as_ref().expect("live client")) {
            None => c
                .as_ref()
                .expect("live client")
                .exit_with_message(c"access not allowed".to_owned(), None),
            Some(entry) if entry.flags & SERVER_ACL_READONLY != 0 => c
                .as_ref()
                .expect("live client")
                .update_flags(CLIENT_READONLY as uint64_t, 0),
            Some(_) => c
                .as_ref()
                .expect("live client")
                .update_flags(0, !(!CLIENT_READONLY as uint64_t)),
        }
        registry_c_owner =
            ClientRegistry::next(registry_c_owner.as_ref().expect("current registry client"));
        c = registry_c_owner.clone();
    }
}
pub unsafe fn server_acl_init() {
    server_acl_entries::clear();
    if getuid() != 0 as __uid_t {
        server_acl_allow(0 as id_t, 0 as ::core::ffi::c_int);
    }
    server_acl_allow(getuid() as id_t, 0 as ::core::ffi::c_int);
}
pub fn server_acl_find(mut id: id_t, mut flags: ::core::ffi::c_int) -> ::core::ffi::c_int {
    server_acl_entries::find(id, flags).is_some() as ::core::ffi::c_int
}
pub unsafe fn server_acl_display(item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>) {
    let mut pw: *mut passwd = ::core::ptr::null_mut::<passwd>();
    let mut gr: *mut group = ::core::ptr::null_mut::<group>();
    let mut name: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut type_0: ::core::ffi::c_char = 0;
    let mut current_block_12: u64;
    for loop_0 in server_acl_entries::snapshot() {
        if !loop_0.flags & SERVER_ACL_IS_GROUP != 0 {
            if loop_0.id == 0 as id_t {
                current_block_12 = 14916268686031723178;
            } else {
                pw = getpwuid(loop_0.id as __uid_t);
                if !pw.is_null() {
                    name = (*pw).pw_name;
                } else {
                    name = c"unknown".as_ptr();
                }
                type_0 = 'U' as i32 as ::core::ffi::c_char;
                current_block_12 = 11050875288958768710;
            }
        } else {
            gr = getgrgid(loop_0.id as __gid_t);
            if !gr.is_null() {
                name = (*gr).gr_name;
            } else {
                name = c"unknown".as_ptr();
            }
            type_0 = 'G' as i32 as ::core::ffi::c_char;
            current_block_12 = 11050875288958768710;
        }
        if current_block_12 == 11050875288958768710 {
            if loop_0.flags & SERVER_ACL_READONLY != 0 {
                cmdq_print(item_handle, |out| {
                    write_cstr(out, nullable_cstr(name))?;
                    out.write_all(b" (")?;
                    out.write_all(&[(type_0 as ::core::ffi::c_int) as u8])?;
                    out.write_all(b",R)")
                });
            } else {
                cmdq_print(item_handle, |out| {
                    write_cstr(out, nullable_cstr(name))?;
                    out.write_all(b" (")?;
                    out.write_all(&[(type_0 as ::core::ffi::c_int) as u8])?;
                    out.write_all(b",W)")
                });
            }
        }
    }
}
pub fn server_acl_allow(mut id: id_t, mut flags: ::core::ffi::c_int) {
    server_acl_entries::insert(server_acl_entry {
        id,
        flags: flags & SERVER_ACL_IS_GROUP,
    });
}
pub fn server_acl_deny(mut id: id_t, mut flags: ::core::ffi::c_int) {
    if server_acl_entries::remove(id, flags) {
        server_acl_update();
    }
}
pub fn server_acl_allow_write(mut id: id_t, mut flags: ::core::ffi::c_int) {
    if server_acl_entries::update(id, flags, |entry| entry.flags &= !SERVER_ACL_READONLY) {
        server_acl_update();
    }
}
pub fn server_acl_deny_write(mut id: id_t, mut flags: ::core::ffi::c_int) {
    if server_acl_entries::update(id, flags, |entry| entry.flags |= SERVER_ACL_READONLY) {
        server_acl_update();
    }
}
pub fn server_acl_join(c: &ClientRef) -> ::core::ffi::c_int {
    let Some(entry) = server_acl_check(c) else {
        return 0 as ::core::ffi::c_int;
    };
    if entry.flags & SERVER_ACL_READONLY != 0 {
        c.update_flags(CLIENT_READONLY as u64, 0);
    }
    1 as ::core::ffi::c_int
}
