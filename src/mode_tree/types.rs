//! Authoritative mode_tree model declarations.

use crate::shared::abi::{size_t, u_int, uint64_t};
use crate::shared::client::{ClientRef, ClientWeak};
use crate::shared::key::key_code;
use crate::shared::menu::menu_item;
use crate::shared::pane::window_pane;
use crate::shared::prompt::{prompt_free_cb, prompt_key_result, prompt_result};
use crate::shared::screen::screen;
use crate::shared::screen_write::screen_write_ctx;
use crate::shared::sort::sort_criteria;
use crate::window_buffer::window_buffer_itemdata;
use crate::window_client::window_client_itemdata;
use crate::window_customize::window_customize_itemdata;
use crate::window_tree::window_tree_itemdata;
use std::cell::UnsafeCell;
use std::rc::{Rc, Weak};

#[derive(Copy, Clone)]
pub struct mode_tree_help_info {
    pub width: u_int,
    pub item: &'static ::std::ffi::CStr,
    pub lines: &'static [&'static ::std::ffi::CStr],
}

pub struct mode_tree_data {
    pub dead: ::core::ffi::c_int,
    pub wp: Weak<UnsafeCell<window_pane>>,
    pub menu: &'static [menu_item<'static>],
    pub sort_crit: sort_criteria,
    pub view_name: Option<&'static ::std::ffi::CStr>,
    pub buildcb: mode_tree_build_cb,
    pub drawcb: mode_tree_draw_cb,
    pub searchcb: mode_tree_search_cb,
    pub menucb: mode_tree_menu_cb,
    pub heightcb: mode_tree_height_cb,
    pub keycb: mode_tree_key_cb,
    pub swapcb: mode_tree_swap_cb,
    pub sortcb: mode_tree_sort_cb,
    pub helpcb: mode_tree_help_cb,
    pub children: mode_tree_list,
    pub saved: mode_tree_list,
    pub lines: Vec<mode_tree_line>,
    pub depth: u_int,
    pub maxdepth: u_int,
    pub width: u_int,
    pub height: u_int,
    pub offset: u_int,
    pub current: u_int,
    pub screen: screen,
    pub prompt: Option<refbox::RefBox<crate::shared::prompt::prompt>>,
    pub prompt_data: refbox::Weak<super::types::mode_tree_prompt>,
    pub prompt_cx: u_int,
    pub prompt_top: ::core::ffi::c_int,
    pub preview: ::core::ffi::c_int,
    pub search: Option<std::ffi::CString>,
    pub filter: Option<std::ffi::CString>,
    pub no_matches: ::core::ffi::c_int,
    pub search_dir: mode_tree_search_dir,
    pub search_icase: ::core::ffi::c_int,
    pub help: ::core::ffi::c_int,
}

impl Default for mode_tree_data {
    fn default() -> Self {
        Self {
            dead: 0,
            wp: Weak::new(),
            menu: &[],
            sort_crit: sort_criteria {
                order: 0,
                reversed: 0,
                order_seq: &[],
            },
            view_name: None,
            buildcb: None,
            drawcb: None,
            searchcb: None,
            menucb: None,
            heightcb: None,
            keycb: None,
            swapcb: None,
            sortcb: None,
            helpcb: None,
            children: mode_tree_list::default(),
            saved: mode_tree_list::default(),
            lines: Vec::new(),
            depth: 0,
            maxdepth: 0,
            width: 0,
            height: 0,
            offset: 0,
            current: 0,
            screen: screen::empty(),
            prompt: None,
            prompt_data: refbox::Weak::new(),
            prompt_cx: 0,
            prompt_top: 0,
            preview: 0,
            search: None,
            filter: None,
            no_matches: 0,
            search_dir: 0,
            search_icase: 0,
            help: 0,
        }
    }
}

pub type mode_tree_search_dir = ::core::ffi::c_uint;

/// The prompt cleanup closure owns this record; all other handles observe it.
pub struct mode_tree_prompt {
    /// Taken at logical cleanup before dropping the callback record.
    pub mtd: Option<Rc<UnsafeCell<mode_tree_data>>>,
    pub c: ClientWeak,
    pub inputcb: mode_tree_prompt_input_cb,
    pub freecb: prompt_free_cb,
}

pub type mode_tree_prompt_input_cb = Option<
    Box<dyn FnMut(Option<&ClientRef>, Option<&std::ffi::CStr>, prompt_key_result) -> prompt_result>,
>;

/// Mode lists own payloads. Rows carry weak identities; actions copy a snapshot
/// before dispatch so rebuilding or drawing the list cannot conflict with a borrow.
#[derive(Clone, Default)]
pub enum ModeTreeItemData {
    #[default]
    None,
    Buffer(refbox::Weak<window_buffer_itemdata>),
    Client(refbox::Weak<window_client_itemdata>),
    Customize(refbox::Weak<window_customize_itemdata>),
    Tree(refbox::Weak<window_tree_itemdata>),
}

/// An action's independent copy, paired with the original nonowning identity.
/// The copy permits reentrant drawing and rebuilds without retaining a registry
/// record or holding a RefBox borrow across callbacks.
pub struct ModeTreeItemSnapshot<T> {
    identity: refbox::Weak<T>,
    value: T,
}

impl<T> std::ops::Deref for ModeTreeItemSnapshot<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T: Clone> ModeTreeItemSnapshot<T> {
    fn read(identity: &refbox::Weak<T>) -> Option<Self> {
        let value = match identity.try_borrow_mut() {
            Ok(value) => value.clone(),
            Err(refbox::BorrowError::Dropped) => return None,
            Err(refbox::BorrowError::Borrowed) => panic!("mode payload already borrowed"),
        };
        Some(Self {
            identity: identity.clone(),
            value,
        })
    }
}

impl ModeTreeItemData {
    pub fn same_identity(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::None, Self::None) => true,
            (Self::Buffer(a), Self::Buffer(b)) => a == b,
            (Self::Client(a), Self::Client(b)) => a == b,
            (Self::Customize(a), Self::Customize(b)) => a == b,
            (Self::Tree(a), Self::Tree(b)) => a == b,
            _ => false,
        }
    }

    pub fn is_buffer(&self, item: &ModeTreeItemSnapshot<window_buffer_itemdata>) -> bool {
        matches!(self, Self::Buffer(handle) if *handle == item.identity)
    }

    pub fn as_buffer(&self) -> Option<ModeTreeItemSnapshot<window_buffer_itemdata>> {
        match self {
            Self::Buffer(item) => ModeTreeItemSnapshot::read(item),
            _ => None,
        }
    }
    pub fn is_client(&self, item: &ModeTreeItemSnapshot<window_client_itemdata>) -> bool {
        matches!(self, Self::Client(handle) if *handle == item.identity)
    }

    pub fn as_client(&self) -> Option<ModeTreeItemSnapshot<window_client_itemdata>> {
        match self {
            Self::Client(item) => ModeTreeItemSnapshot::read(item),
            _ => None,
        }
    }
    pub fn is_customize(&self, item: &ModeTreeItemSnapshot<window_customize_itemdata>) -> bool {
        matches!(self, Self::Customize(handle) if *handle == item.identity)
    }

    pub fn as_customize(&self) -> Option<ModeTreeItemSnapshot<window_customize_itemdata>> {
        match self {
            Self::Customize(item) => ModeTreeItemSnapshot::read(item),
            _ => None,
        }
    }

    pub fn as_tree(&self) -> Option<ModeTreeItemSnapshot<window_tree_itemdata>> {
        match self {
            Self::Tree(item) => ModeTreeItemSnapshot::read(item),
            _ => None,
        }
    }
}

/// A row identity. Only its containing list owns the allocation.
#[derive(Clone, PartialEq, Eq)]
pub struct ModeTreeItemRef(refbox::Weak<mode_tree_item>);

impl Default for ModeTreeItemRef {
    fn default() -> Self {
        Self(refbox::Weak::new())
    }
}

impl ModeTreeItemRef {
    pub(crate) fn observe(owner: &refbox::RefBox<mode_tree_item>) -> Self {
        Self(owner.downgrade())
    }

    pub fn is_alive(&self) -> bool {
        self.0.is_alive()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn try_borrow(&self) -> Option<refbox::Borrow<'_, mode_tree_item>> {
        match self.0.try_borrow_mut() {
            Ok(row) => Some(row),
            Err(refbox::BorrowError::Dropped) => None,
            Err(refbox::BorrowError::Borrowed) => panic!("mode row already borrowed"),
        }
    }

    pub fn borrow(&self) -> refbox::Borrow<'_, mode_tree_item> {
        self.try_borrow().expect("live mode row")
    }

    pub fn borrow_mut(&self) -> refbox::Borrow<'_, mode_tree_item> {
        self.borrow()
    }
}

#[derive(Clone)]
pub struct mode_tree_line {
    pub item: ModeTreeItemRef,
    pub depth: u_int,
    pub last: ::core::ffi::c_int,
    pub flat: ::core::ffi::c_int,
}

pub struct mode_tree_item {
    pub parent: ModeTreeItemRef,
    pub itemdata: ModeTreeItemData,
    pub line: u_int,
    pub key: key_code,
    pub keystr: Option<std::ffi::CString>,
    pub keylen: size_t,
    pub tag: uint64_t,
    pub name: std::ffi::CString,
    pub text: Option<std::ffi::CString>,
    pub expanded: ::core::ffi::c_int,
    pub tagged: ::core::ffi::c_int,
    pub draw_as_parent: ::core::ffi::c_int,
    pub no_tag: ::core::ffi::c_int,
    pub align: ::core::ffi::c_int,
    pub children: mode_tree_list,
}

impl mode_tree_item {
    pub fn empty() -> Self {
        Self {
            parent: Default::default(),
            itemdata: Default::default(),
            line: Default::default(),
            key: Default::default(),
            keystr: Default::default(),
            keylen: Default::default(),
            tag: 0,
            name: std::ffi::CString::default(),
            text: Default::default(),
            expanded: Default::default(),
            tagged: Default::default(),
            draw_as_parent: Default::default(),
            no_tag: Default::default(),
            align: Default::default(),
            children: Default::default(),
        }
    }
}

/// Ordered sole owners. Parent links, visible lines, and traversal observe weakly.
#[derive(Default)]
pub struct mode_tree_list {
    pub(crate) items: Vec<refbox::RefBox<mode_tree_item>>,
}

impl mode_tree_list {
    pub(crate) fn snapshot(&self) -> Vec<ModeTreeItemRef> {
        self.items.iter().map(ModeTreeItemRef::observe).collect()
    }

    pub(crate) fn first(&self) -> Option<ModeTreeItemRef> {
        self.items.first().map(ModeTreeItemRef::observe)
    }

    pub(crate) fn last(&self) -> Option<ModeTreeItemRef> {
        self.items.last().map(ModeTreeItemRef::observe)
    }

    pub(crate) fn position(&self, item: &ModeTreeItemRef) -> usize {
        self.items
            .iter()
            .position(|candidate| item.0.is(candidate))
            .expect("mode tree item belongs to its parent list")
    }

    pub(crate) fn next(&self, item: &ModeTreeItemRef) -> Option<ModeTreeItemRef> {
        self.items
            .get(self.position(item) + 1)
            .map(ModeTreeItemRef::observe)
    }

    pub(crate) fn previous(&self, item: &ModeTreeItemRef) -> Option<ModeTreeItemRef> {
        self.position(item)
            .checked_sub(1)
            .and_then(|position| self.items.get(position))
            .map(ModeTreeItemRef::observe)
    }
}

pub type mode_tree_help_cb = Option<fn() -> mode_tree_help_info>;

pub type mode_tree_sort_cb = Option<fn(&mut sort_criteria)>;

pub type mode_tree_swap_cb =
    Option<Box<dyn FnMut(&ModeTreeItemData, &ModeTreeItemData, &mut sort_criteria) -> bool>>;

pub type mode_tree_key_cb = Option<Box<dyn FnMut(&ModeTreeItemData, u_int) -> key_code>>;

pub type mode_tree_height_cb = Option<Box<dyn FnMut(u_int) -> u_int>>;

pub type mode_tree_menu_cb = Option<Box<dyn FnMut(&ClientRef, key_code)>>;

pub type mode_tree_search_cb =
    Option<Box<dyn FnMut(&ModeTreeItemData, &std::ffi::CStr, bool) -> bool>>;

pub type mode_tree_draw_cb =
    Option<Box<dyn FnMut(&ModeTreeItemData, &mut screen_write_ctx, u_int, u_int)>>;

pub type mode_tree_build_cb = Option<
    Box<
        dyn FnMut(
            &mut sort_criteria,
            Option<uint64_t>,
            Option<&std::ffi::CStr>,
        ) -> Option<uint64_t>,
    >,
>;
