//! Operations on retained session holders. Cleanup remains explicit.
use super::*;
use crate::src::shared::session::SessionRef;
use crate::src::shared::spawn::spawn_context;
use crate::src::shared::window::WindowRef;
use std::time::SystemTime;

/// The caller must preserve the legacy single-threaded borrow and logical
/// lifetime rules. No model/component reference may survive a callback.
pub trait Session {
    /// Publish attachment counts and status caches before the size pass.
    unsafe fn recalculate_attachment_status()
    where
        Self: Sized;
    /// Deliver one window's alerts across its linked sessions.
    unsafe fn deliver_window_alerts(window: &WindowRef) -> i32
    where
        Self: Sized;
    unsafe fn create(
        prefix: Option<&CStr>,
        name: Option<&CStr>,
        cwd: &CStr,
        environment: Box<environ>,
        options: Option<Box<options>>,
        termios: Option<&termios>,
    ) -> Self
    where
        Self: Sized;
    unsafe fn find_by_id_str(id: &CStr) -> Option<Self>
    where
        Self: Sized;
    unsafe fn sorted(criteria: &sort_criteria) -> Vec<Self>
    where
        Self: Sized;
    /// Release this owner now, preserving the release diagnostics.
    unsafe fn release(self, from: &CStr)
    where
        Self: Sized;
    unsafe fn adjacent_session(&self, criteria: &sort_criteria, previous: bool) -> Option<Self>
    where
        Self: Sized;
    unsafe fn select_index(&self, index: i32) -> i32;
    unsafe fn select_adjacent_window(&self, previous: bool, alert: bool) -> i32;
    unsafe fn select_last_window(&self) -> i32;
    unsafe fn window_linked_outside_group(session: Option<&Self>, window: &WindowRef) -> bool
    where
        Self: Sized;
    type Environment<'a>: std::ops::Deref<Target = environ>
    where
        Self: 'a;
    type EnvironmentMut<'a>: std::ops::DerefMut<Target = environ>
    where
        Self: 'a;
    /// Scoped component borrows. Release the guard before model calls, callbacks,
    /// or environment replacement. These may become Ref/RefMut::map of Session.
    unsafe fn borrow_environment(&self) -> Option<Self::Environment<'_>>;
    unsafe fn borrow_environment_mut(&self) -> Option<Self::EnvironmentMut<'_>>;

    /// Apply update-environment patterns without exposing either component.
    unsafe fn update_environment(&self, source: &environ);

    /// Registry liveness is distinct from retained allocation liveness.
    unsafe fn is_registered(&self) -> bool;
    unsafe fn contains_window(&self, window: &WindowRef) -> bool;
    unsafe fn id(&self) -> u32;
    unsafe fn name(&self) -> CString;
    unsafe fn rename(&self, name: &CStr) -> Result<(), CString>;
    unsafe fn activity_time(&self) -> SystemTime;
    unsafe fn is_attached(&self) -> bool;
    /// Number of attached clients, including multiple clients of one group member.
    unsafe fn attached_count(&self) -> u32;
    /// Rebuild this Session's owned winlinks from a distinct, live source.
    /// The caller must pass different source and destination identities.
    /// Live source traversal, selection/history remapping and link notifications
    /// retain their order;
    /// neither Session is borrowed during callbacks or old-window cleanup.
    /// Linked callbacks must preserve the current source and replacement links
    /// through the subsequent alert copy, as required by their logical lifetimes.
    unsafe fn synchronize_windows_from(&self, source: &SessionRef);
    unsafe fn current_winlink(&self) -> refbox::Weak<winlink>;
    /// Observe MRU history without selecting it (notably command target `!`).
    unsafe fn last_winlink(&self) -> refbox::Weak<winlink>;
    /// Read only. Do not mutate entries, reenter, or let references/pointers escape.
    unsafe fn with_winlinks<R>(&self, read: impl FnOnce(&winlinks) -> R) -> R;
    /// Remove only MRU membership, before releasing a link's window.
    unsafe fn forget_winlink(&self, link: refbox::Weak<winlink>);
    /// Reserve an index adjacent to a link by shifting consecutive indices.
    unsafe fn shuffle_window(&self, link: refbox::Weak<winlink>, before: bool) -> i32;
    /// Remove a replaced link without selecting a successor; return whether it was current.
    unsafe fn remove_replaced_window(&self, link: refbox::Weak<winlink>) -> bool;
    unsafe fn select_winlink(&self, link: refbox::Weak<winlink>) -> i32;
    unsafe fn detach_window(&self, link: refbox::Weak<winlink>) -> i32;
    /// Adopt an existing window, including break-pane's already-running pane.
    unsafe fn attach_window(
        &self,
        window: &WindowRef,
        index: i32,
    ) -> Result<refbox::Weak<winlink>, CString>;
    unsafe fn link_window(
        &self,
        source: &SessionRef,
        link: refbox::Weak<winlink>,
        index: i32,
        replace: bool,
        select: bool,
    ) -> Result<(), CString>;
    unsafe fn spawn_window(
        &self,
        context: &mut spawn_context,
    ) -> Result<refbox::Weak<winlink>, CString>;
    unsafe fn renumber_windows(&self);
    unsafe fn update_activity(&self, from: Option<SystemTime>);
    unsafe fn update_history(&self);
    unsafe fn on_attached(&self);
    unsafe fn theme_changed(&self);
    unsafe fn status_layout(&self) -> (i32, u32);
    unsafe fn join_group(&self, name: &CStr);
    /// Scopes cannot reenter model code, run callbacks, change component parents,
    /// free components, or allow references/pointers to escape.
    unsafe fn with_options_mut<R>(&self, edit: impl FnOnce(&mut options) -> R) -> R;
    unsafe fn with_environment_mut<R>(&self, edit: impl FnOnce(&mut environ) -> R) -> R;
    unsafe fn set_cwd(&self, cwd: Option<CString>);
    unsafe fn cwd(&self) -> Option<CString>;
    unsafe fn termios(&self) -> Option<termios>;
    /// Evaluate a session builtin in a context already targeting this holder.
    unsafe fn format_value(
        &self,
        key: &CStr,
        context: &mut crate::src::shared::format::format_tree,
    ) -> Option<crate::src::format::FormatValue>;
    unsafe fn destroy(&self, notify: bool, from: &CStr);
}

impl Session for SessionRef {
    unsafe fn recalculate_attachment_status() {
        recalculate_size_state();
    }
    unsafe fn deliver_window_alerts(window: &WindowRef) -> i32 {
        alerts_check_all(window)
    }
    unsafe fn create(
        prefix: Option<&CStr>,
        name: Option<&CStr>,
        cwd: &CStr,
        environment: Box<environ>,
        options: Option<Box<options>>,
        termios: Option<&termios>,
    ) -> Self {
        session_create(prefix, name, cwd, environment, options, termios)
    }
    unsafe fn find_by_id_str(id: &CStr) -> Option<Self> {
        session_find_by_id_str(id)
    }
    unsafe fn sorted(criteria: &sort_criteria) -> Vec<Self> {
        sort_get_sessions(criteria)
    }
    unsafe fn release(self, from: &CStr) {
        session_remove_ref(self, from);
    }
    unsafe fn adjacent_session(&self, criteria: &sort_criteria, previous: bool) -> Option<Self> {
        session_adjacent(Some(self), criteria, previous)
    }
    unsafe fn select_index(&self, index: i32) -> i32 {
        session_select(self, index)
    }
    unsafe fn select_adjacent_window(&self, previous: bool, alert: bool) -> i32 {
        if previous {
            session_previous(self, alert as i32)
        } else {
            session_next(self, alert as i32)
        }
    }
    unsafe fn select_last_window(&self) -> i32 {
        session_last(self)
    }

    unsafe fn window_linked_outside_group(session: Option<&Self>, window: &WindowRef) -> bool {
        session_is_linked(session, window)
    }

    type Environment<'a> = &'a environ;
    type EnvironmentMut<'a> = &'a mut environ;
    unsafe fn borrow_environment(&self) -> Option<Self::Environment<'_>> {
        (*self.get()).environ.as_deref()
    }
    unsafe fn borrow_environment_mut(&self) -> Option<Self::EnvironmentMut<'_>> {
        (*self.get()).environ.as_deref_mut()
    }

    unsafe fn update_environment(&self, source: &environ) {
        let state = &mut *self.get();
        crate::src::environ::environ_update(
            state.options.as_deref_mut().expect("session options"),
            source,
            state.environ.as_deref_mut().expect("session environment"),
        );
    }

    unsafe fn is_registered(&self) -> bool {
        sessions::resolve(&Rc::downgrade(self)).is_some()
    }
    unsafe fn contains_window(&self, window: &WindowRef) -> bool {
        use crate::src::window::Window;
        let observer = Rc::downgrade(self);
        let mut link = window.next_winlink(None);
        while link.is_alive() {
            if link.get_unchecked().session.ptr_eq(&observer) {
                return true;
            }
            link = window.next_winlink(Some(link));
        }
        false
    }
    unsafe fn id(&self) -> u32 {
        (*self.get()).id
    }
    unsafe fn name(&self) -> CString {
        (*self.get()).name.clone()
    }

    unsafe fn rename(&self, name: &CStr) -> Result<(), CString> {
        use crate::src::tmux::{check_name, clean_name_cstring};
        if !check_name(name) {
            let mut message = b"invalid session name: ".to_vec();
            message.extend_from_slice(name.to_bytes());
            return Err(CString::new(message).expect("C string name"));
        }
        let new_name = clean_name_cstring(name, 0).expect("validated session name");
        let old_name = self.name();
        if old_name == new_name {
            return Ok(());
        }
        if sessions::find(&new_name).is_some() {
            let mut message = b"duplicate session: ".to_vec();
            message.extend_from_slice(new_name.as_bytes());
            return Err(CString::new(message).expect("C string name"));
        }
        let mut find = cmd_find_state {
            flags: 0,
            s: Default::default(),
            wl: Default::default(),
            w: Default::default(),
            wp: Default::default(),
            idx: 0,
        };
        find = cmd_find_state::find_from_session(self, 0);
        let mut payload = event_payload_create();
        event_payload_set_target(&mut payload, &find);
        event_payload_set_session(&mut payload, c"session", self.clone());
        event_payload_set_string(&mut payload, c"old_name", |out| {
            out.write_all(old_name.as_bytes())
        });
        event_payload_set_string(&mut payload, c"new_name", |out| {
            out.write_all(new_name.as_bytes())
        });
        let owner = sessions::remove(self).expect("registered session owner");
        drop(session_replace_name(&mut *self.get(), new_name));
        sessions::insert(owner);
        crate::src::server_fn::server_status_session(self);
        events_fire(c"session-renamed", payload);
        Ok(())
    }

    unsafe fn activity_time(&self) -> SystemTime {
        (*self.get()).activity_time
    }
    unsafe fn is_attached(&self) -> bool {
        (*self.get()).attached != 0
    }
    unsafe fn attached_count(&self) -> u32 {
        (*self.get()).attached
    }
    unsafe fn synchronize_windows_from(&self, source: &SessionRef) {
        session_synchronize_windows(source, self);
    }
    unsafe fn current_winlink(&self) -> refbox::Weak<winlink> {
        (*self.get()).curw.clone()
    }
    unsafe fn last_winlink(&self) -> refbox::Weak<winlink> {
        (*self.get()).lastw.front().cloned().unwrap_or_default()
    }
    unsafe fn with_winlinks<R>(&self, read: impl FnOnce(&winlinks) -> R) -> R {
        read(&(*self.get()).windows)
    }
    unsafe fn forget_winlink(&self, link: refbox::Weak<winlink>) {
        winlink_stack_remove(&raw mut (*self.get()).lastw, link);
    }
    unsafe fn shuffle_window(&self, link: refbox::Weak<winlink>, before: bool) -> i32 {
        if !link.is_alive() || link.get_unchecked().idx == i32::MAX {
            return -1;
        }
        let index = link.get_unchecked().idx + if before { 0 } else { 1 };
        let state = &mut *self.get();
        let mut last = index;
        while last < i32::MAX && winlink_find_by_index(&state.windows, last).is_alive() {
            last += 1;
        }
        if last == i32::MAX {
            return -1;
        }
        while last > index {
            let link = winlink_find_by_index(&state.windows, last - 1);
            crate::src::window::winlinks_reindex(&mut state.windows, link, last);
            last -= 1;
        }
        index
    }
    unsafe fn remove_replaced_window(&self, mut link: refbox::Weak<winlink>) -> bool {
        // Notification precedes alert/history clearing and explicit window release.
        events_fire_winlink(c"window-unlinked", link.clone());
        link.get_mut_unchecked().flags &= !WINLINK_ALERTFLAGS;
        self.forget_winlink(link.clone());
        winlink_remove(&raw mut (*self.get()).windows, link.clone());
        let was_current = link == self.current_winlink();
        if was_current {
            (*self.get()).curw = refbox::Weak::new();
        }
        was_current
    }
    unsafe fn select_winlink(&self, link: refbox::Weak<winlink>) -> i32 {
        session_set_current(self, link)
    }
    unsafe fn detach_window(&self, link: refbox::Weak<winlink>) -> i32 {
        session_detach(self, link)
    }
    unsafe fn attach_window(
        &self,
        window: &WindowRef,
        index: i32,
    ) -> Result<refbox::Weak<winlink>, CString> {
        session_attach(self, window, index)
    }
    unsafe fn link_window(
        &self,
        source: &SessionRef,
        link: refbox::Weak<winlink>,
        index: i32,
        replace: bool,
        select: bool,
    ) -> Result<(), CString> {
        crate::src::server_fn::server_link_window(
            source,
            link,
            self,
            index,
            replace as i32,
            select as i32,
        )
    }
    unsafe fn spawn_window(
        &self,
        context: &mut spawn_context,
    ) -> Result<refbox::Weak<winlink>, CString> {
        assert!(
            context.s.ptr_eq(&Rc::downgrade(self)),
            "spawn context session must match receiver"
        );
        let mut cause = None;
        let link = super::spawn::spawn_window(context, &mut cause);
        if link.is_alive() {
            Ok(link)
        } else {
            Err(cause.expect("failed window spawn supplies a cause"))
        }
    }
    unsafe fn renumber_windows(&self) {
        session_renumber_windows(self);
    }
    unsafe fn update_history(&self) {
        session_update_history(&*self.get());
    }
    unsafe fn update_activity(&self, from: Option<SystemTime>) {
        session_update_activity(self, from);
    }
    unsafe fn theme_changed(&self) {
        session_theme_changed(Some(&*self.get()));
    }
    unsafe fn on_attached(&self) {
        self.update_activity(None);
        session_theme_changed(Some(&*self.get()));
        (*self.get()).last_attached_time = SystemTime::now();
        self.current_winlink().get_mut_unchecked().flags &= !WINLINK_ALERTFLAGS;
        crate::src::alerts::alerts_check_session(self);
    }
    unsafe fn status_layout(&self) -> (i32, u32) {
        ((*self.get()).statusat, (*self.get()).statuslines)
    }
    unsafe fn join_group(&self, name: &CStr) {
        let group = session_groups::find_or_create(name);
        session_group_add(&group, self);
        session_group_synchronize_to(self);
    }
    unsafe fn with_options_mut<R>(&self, edit: impl FnOnce(&mut options) -> R) -> R {
        edit(
            (*self.get())
                .options
                .as_deref_mut()
                .expect("session options"),
        )
    }
    unsafe fn with_environment_mut<R>(&self, edit: impl FnOnce(&mut environ) -> R) -> R {
        let mut environment = self.borrow_environment_mut().expect("session environment");
        edit(environment)
    }
    unsafe fn set_cwd(&self, cwd: Option<CString>) {
        session_set_cwd(&mut *self.get(), cwd);
    }
    unsafe fn cwd(&self) -> Option<CString> {
        (*self.get()).cwd.clone()
    }
    unsafe fn termios(&self) -> Option<termios> {
        (*self.get()).tio.as_deref().copied()
    }
    unsafe fn format_value(
        &self,
        key: &CStr,
        context: &mut crate::src::shared::format::format_tree,
    ) -> Option<crate::src::format::FormatValue> {
        super::format::format_value(self, key, context)
    }
    unsafe fn destroy(&self, notify: bool, from: &CStr) {
        session_destroy(self, notify as i32, from);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joining_a_group_creates_it_once_and_keeps_a_single_membership() {
        use crate::src::session_group::{session_group_count, session_group_remove};
        unsafe {
            let session = session::new();
            assert!(!session_groups::find(c"joined").is_alive());
            session.join_group(c"joined");
            let group = session_groups::find(c"joined");
            assert!(group.is_alive());
            assert_eq!(
                session_groups::for_session(&std::rc::Rc::downgrade(&session)),
                group
            );
            assert_eq!(session_group_count(&group), 1);
            assert_eq!(
                crate::src::session::session_group_name(&group).as_c_str(),
                c"joined"
            );
            // Joining by the same name finds the existing group again.
            session.join_group(c"joined");
            assert_eq!(session_groups::find(c"joined"), group);
            assert_eq!(session_group_count(&group), 1);
            // Leaving as the last member destroys the group.
            session_group_remove(&session);
            assert!(!session_groups::find(c"joined").is_alive());
        }
    }

    #[test]
    fn rename_rejects_invalid_utf8_before_touching_the_registry() {
        unsafe {
            let session = session::new();
            let invalid = CString::new(vec![0xff]).unwrap();
            assert_eq!(
                session.rename(&invalid).unwrap_err().as_bytes(),
                b"invalid session name: \xff"
            );
            assert_eq!(session.name().as_bytes(), b"");
        }
    }

    #[test]
    fn updating_environment_uses_patterns_and_clears_missing_variables() {
        unsafe {
            let owner = session::new();
            let mut options = crate::src::options::options_create(None);
            let definition = crate::src::options_table::options_table
                .iter()
                .find(|definition| definition.name == Some(c"update-environment"))
                .unwrap();
            crate::src::options::options_default(&mut *options, definition);
            (*owner.get()).options = Some(options);
            (*owner.get()).environ = Some(crate::src::environ::environ_create());
            owner.with_environment_mut(|env| {
                env.set(b"DISPLAY", 0, b"old").unwrap();
                env.set(b"SSH_AUTH_SOCK", 0, b"old-socket").unwrap();
                env.set(b"UNCHANGED", 0, b"keep").unwrap();
            });
            let mut source = environ::default();
            source.set(b"DISPLAY", 0, b"new").unwrap();
            source.set(b"UNCHANGED", 0, b"ignore").unwrap();
            owner.update_environment(&source);
            {
                let env = owner.borrow_environment().unwrap();
                assert_eq!(env.find(c"DISPLAY").unwrap().value(), Some(c"new"));
                assert_eq!(env.find(c"SSH_AUTH_SOCK").unwrap().value(), None);
                assert_eq!(env.find(c"UNCHANGED").unwrap().value(), Some(c"keep"));
            }
            // The read borrow has ended before the next mutation.
            owner.with_environment_mut(|env| env.unset_cstr(c"DISPLAY"));
            assert!(owner
                .borrow_environment()
                .unwrap()
                .find(c"DISPLAY")
                .is_none());
        }
    }

    #[test]
    fn status_layout_is_cached_until_the_sizing_pass_publishes_it() {
        unsafe {
            let session = session::new();
            let mut options = crate::src::options::options_create(None);
            for key in [c"status", c"status-position"] {
                let definition = crate::src::options_table::options_table
                    .iter()
                    .find(|definition| definition.name == Some(key))
                    .unwrap();
                crate::src::options::options_default(&mut *options, definition);
            }
            (*session.get()).options = Some(options);
            status_update_cache(&mut *session.get());
            let before = session.status_layout();
            session.with_options_mut(|options| {
                crate::src::options::options_set_number(options, c"status", 0);
            });
            assert_eq!(
                session.status_layout(),
                before,
                "configuration edits do not publish the sizing cache"
            );
            status_update_cache(&mut *session.get());
            assert_eq!(session.status_layout(), (-1, 0));
        }
    }
}
