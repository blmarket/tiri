//! Operations on retained session holders. Cleanup remains explicit.
use super::*;
use crate::shared::session::SessionRef;
use crate::shared::spawn::spawn_context;
use crate::shared::window::WindowRef;
use std::time::SystemTime;

/// The caller must preserve the legacy single-threaded borrow and logical
/// lifetime rules. No model/component reference may survive a callback.
pub trait Session {
    /// Publish attachment counts and status caches before the size pass.
    fn recalculate_attachment_status()
    where
        Self: Sized;
    /// Deliver one window's alerts across its linked sessions.
    fn deliver_window_alerts(window: &WindowRef) -> i32
    where
        Self: Sized;
    fn create(
        prefix: Option<&CStr>,
        name: Option<&CStr>,
        cwd: &CStr,
        environment: Box<environ>,
        options: Option<options>,
        termios: Option<&termios>,
    ) -> Self
    where
        Self: Sized;
    fn find_by_id_str(id: &CStr) -> Option<Self>
    where
        Self: Sized;
    fn sorted(criteria: &sort_criteria) -> Vec<Self>
    where
        Self: Sized;
    /// Release this owner now, preserving the release diagnostics.
    fn release(self, from: &CStr)
    where
        Self: Sized;
    fn adjacent_session(&self, criteria: &sort_criteria, previous: bool) -> Option<Self>
    where
        Self: Sized;
    fn select_index(&self, index: i32) -> i32;
    fn select_adjacent_window(&self, previous: bool, alert: bool) -> i32;
    fn select_last_window(&self) -> i32;
    fn window_linked_outside_group(session: Option<&Self>, window: &WindowRef) -> bool
    where
        Self: Sized;
    /// Apply update-environment patterns without exposing either component.
    fn update_environment(&self, source: &environ);

    /// Registry liveness is distinct from retained allocation liveness.
    fn is_registered(&self) -> bool;
    fn contains_window(&self, window: &WindowRef) -> bool;
    fn id(&self) -> u32;
    fn name(&self) -> CString;
    fn rename(&self, name: &CStr) -> Result<(), CString>;
    fn activity_time(&self) -> SystemTime;
    fn is_attached(&self) -> bool;
    /// Number of attached clients, including multiple clients of one group member.
    fn attached_count(&self) -> u32;
    /// Rebuild this Session's owned winlinks from a distinct, live source.
    /// The caller must pass different source and destination identities.
    /// Live source traversal, selection/history remapping and link notifications
    /// retain their order;
    /// neither Session is borrowed during callbacks or old-window cleanup.
    /// Linked callbacks must preserve the current source and replacement links
    /// through the subsequent alert copy, as required by their logical lifetimes.
    fn synchronize_windows_from(&self, source: &SessionRef);
    fn current_winlink(&self) -> refbox::Weak<winlink>;
    /// Observe MRU history without selecting it (notably command target `!`).
    fn last_winlink(&self) -> refbox::Weak<winlink>;
    /// Read only. Do not mutate entries, reenter, or let references/pointers escape.
    fn with_winlinks<R>(&self, read: impl FnOnce(&winlinks) -> R) -> R;
    /// Remove only MRU membership, before releasing a link's window.
    fn forget_winlink(&self, link: refbox::Weak<winlink>);
    /// Reserve an index adjacent to a link by shifting consecutive indices.
    fn shuffle_window(&self, link: refbox::Weak<winlink>, before: bool) -> i32;
    /// Remove a replaced link without selecting a successor; return whether it was current.
    fn remove_replaced_window(&self, link: refbox::Weak<winlink>) -> bool;
    fn select_winlink(&self, link: refbox::Weak<winlink>) -> i32;
    fn detach_window(&self, link: refbox::Weak<winlink>) -> i32;
    /// Adopt an existing window, including break-pane's already-running pane.
    fn attach_window(
        &self,
        window: &WindowRef,
        index: i32,
    ) -> Result<refbox::Weak<winlink>, CString>;
    unsafe fn spawn_window(
        &self,
        context: &mut spawn_context,
    ) -> Result<refbox::Weak<winlink>, CString>;
    fn renumber_windows(&self);
    fn update_activity(&self, from: Option<SystemTime>);
    fn update_history(&self);
    fn on_attached(&self);
    fn theme_changed(&self);
    fn status_layout(&self) -> (i32, u32);
    /// Borrow the option table through the model. Borrowing it again while held panics.
    fn borrow_options_mut(&self) -> std::cell::RefMut<'_, options>;
    /// Edit the environment with the session borrowed; `None` once it is released.
    /// The closure must not reach back into the session.
    fn with_environment_mut<R>(&self, edit: impl FnOnce(&mut environ) -> R) -> Option<R>;
    fn set_cwd(&self, cwd: Option<CString>);
    fn cwd(&self) -> Option<CString>;
    fn termios(&self) -> Option<termios>;
    /// Evaluate a session builtin in a context already targeting this holder.
    fn format_value(
        &self,
        key: &CStr,
        context: &mut crate::shared::format::format_tree,
    ) -> Option<crate::format::FormatValue>;
    fn destroy(&self, notify: bool, from: &CStr);
}

impl Session for SessionRef {
    fn recalculate_attachment_status() {
        recalculate_size_state();
    }
    fn deliver_window_alerts(window: &WindowRef) -> i32 {
        alerts_check_all(window)
    }
    fn create(
        prefix: Option<&CStr>,
        name: Option<&CStr>,
        cwd: &CStr,
        environment: Box<environ>,
        options: Option<options>,
        termios: Option<&termios>,
    ) -> Self {
        session_create(prefix, name, cwd, environment, options, termios)
    }
    fn find_by_id_str(id: &CStr) -> Option<Self> {
        session_find_by_id_str(id)
    }
    fn sorted(criteria: &sort_criteria) -> Vec<Self> {
        sort_get_sessions(criteria)
    }
    fn release(self, from: &CStr) {
        session_remove_ref(self, from);
    }
    fn adjacent_session(&self, criteria: &sort_criteria, previous: bool) -> Option<Self> {
        session_adjacent(Some(self), criteria, previous)
    }
    fn select_index(&self, index: i32) -> i32 {
        session_select(self, index)
    }
    fn select_adjacent_window(&self, previous: bool, alert: bool) -> i32 {
        if previous {
            session_previous(self, alert as i32)
        } else {
            session_next(self, alert as i32)
        }
    }
    fn select_last_window(&self) -> i32 {
        session_last(self)
    }

    fn window_linked_outside_group(session: Option<&Self>, window: &WindowRef) -> bool {
        session_is_linked(session, window)
    }

    fn update_environment(&self, source: &environ) {
        // The patterns are read through this session's options, so the
        // environment is lent out of the model for the update.
        let mut environment = self
            .borrow_mut()
            .environ
            .take()
            .expect("session environment");
        crate::environ::environ_update(
            &crate::options::OptionsScope::Session(Rc::downgrade(self)),
            source,
            &mut environment,
        );
        self.borrow_mut().environ = Some(environment);
    }

    fn is_registered(&self) -> bool {
        sessions::resolve(&Rc::downgrade(self)).is_some()
    }
    fn contains_window(&self, window: &WindowRef) -> bool {
        use crate::window::Window;
        let observer = Rc::downgrade(self);
        let mut link = window.next_winlink(None);
        while link.is_alive() {
            if link
                .try_borrow()
                .expect("live winlink")
                .session
                .ptr_eq(&observer)
            {
                return true;
            }
            link = window.next_winlink(Some(link));
        }
        false
    }
    fn id(&self) -> u32 {
        self.borrow().id
    }
    fn name(&self) -> CString {
        self.borrow().name.clone()
    }

    fn rename(&self, name: &CStr) -> Result<(), CString> {
        use crate::tmux::{check_name, clean_name_cstring};
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
        let replaced = session_replace_name(&mut self.borrow_mut(), new_name);
        drop(replaced);
        sessions::insert(owner);
        crate::server_fn::server_status_session(self);
        events_fire(c"session-renamed", payload);
        Ok(())
    }

    fn activity_time(&self) -> SystemTime {
        self.borrow().activity_time
    }
    fn is_attached(&self) -> bool {
        self.borrow().attached != 0
    }
    fn attached_count(&self) -> u32 {
        self.borrow().attached
    }
    fn synchronize_windows_from(&self, source: &SessionRef) {
        session_synchronize_windows(source, self);
    }
    fn current_winlink(&self) -> refbox::Weak<winlink> {
        self.borrow().curw.clone()
    }
    fn last_winlink(&self) -> refbox::Weak<winlink> {
        self.borrow().lastw.front().cloned().unwrap_or_default()
    }
    fn with_winlinks<R>(&self, read: impl FnOnce(&winlinks) -> R) -> R {
        read(&self.borrow().windows)
    }
    fn forget_winlink(&self, link: refbox::Weak<winlink>) {
        winlink_stack_remove(&mut self.borrow_mut().lastw, link);
    }
    fn shuffle_window(&self, link: refbox::Weak<winlink>, before: bool) -> i32 {
        if !link.is_alive() || link.try_borrow().expect("live winlink").idx == i32::MAX {
            return -1;
        }
        let index = link.try_borrow().expect("live winlink").idx + if before { 0 } else { 1 };
        let state = self.borrow();
        let mut last = index;
        while last < i32::MAX && winlink_find_by_index(&state.windows, last).is_alive() {
            last += 1;
        }
        if last == i32::MAX {
            return -1;
        }
        while last > index {
            let link = winlink_find_by_index(&state.windows, last - 1);
            crate::window::winlinks_reindex(&state.windows, link, last);
            last -= 1;
        }
        index
    }
    fn remove_replaced_window(&self, mut link: refbox::Weak<winlink>) -> bool {
        events_fire_winlink(c"window-unlinked", link.clone());
        link.try_borrow_mut().expect("live winlink").flags &= !WINLINK_ALERTFLAGS;
        self.forget_winlink(link.clone());
        winlink_remove(link.clone());
        let was_current = link == self.current_winlink();
        if was_current {
            self.borrow_mut().curw = refbox::Weak::new();
        }
        was_current
    }
    fn select_winlink(&self, link: refbox::Weak<winlink>) -> i32 {
        session_set_current(self, link)
    }
    fn detach_window(&self, link: refbox::Weak<winlink>) -> i32 {
        session_detach(self, link)
    }
    fn attach_window(
        &self,
        window: &WindowRef,
        index: i32,
    ) -> Result<refbox::Weak<winlink>, CString> {
        session_attach(self, window, index)
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
    fn renumber_windows(&self) {
        session_renumber_windows(self);
    }
    fn update_history(&self) {
        session_update_history(self);
    }
    fn update_activity(&self, from: Option<SystemTime>) {
        session_update_activity(self, from);
    }
    fn theme_changed(&self) {
        session_theme_changed(self);
    }
    fn on_attached(&self) {
        self.update_activity(None);
        session_theme_changed(self);
        self.borrow_mut().last_attached_time = SystemTime::now();
        self.current_winlink()
            .try_borrow_mut()
            .expect("live winlink")
            .flags &= !WINLINK_ALERTFLAGS;
        crate::alerts::alerts_check_session(self);
    }
    fn status_layout(&self) -> (i32, u32) {
        (self.borrow().statusat, self.borrow().statuslines)
    }
    fn borrow_options_mut(&self) -> std::cell::RefMut<'_, options> {
        std::cell::RefMut::map(std::cell::RefCell::borrow_mut(self), |model| {
            model.options.as_mut().expect("live session options")
        })
    }
    fn with_environment_mut<R>(&self, edit: impl FnOnce(&mut environ) -> R) -> Option<R> {
        self.borrow_mut().environ.as_deref_mut().map(edit)
    }
    fn set_cwd(&self, cwd: Option<CString>) {
        session_set_cwd(&mut self.borrow_mut(), cwd);
    }
    fn cwd(&self) -> Option<CString> {
        self.borrow().cwd.clone()
    }
    fn termios(&self) -> Option<termios> {
        self.borrow().tio.as_deref().copied()
    }
    fn format_value(
        &self,
        key: &CStr,
        context: &mut crate::shared::format::format_tree,
    ) -> Option<crate::format::FormatValue> {
        super::format::format_value(self, key, context)
    }
    fn destroy(&self, notify: bool, from: &CStr) {
        session_destroy(self, notify as i32, from);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_rejects_invalid_utf8_before_touching_the_registry() {
        let session = session::new();
        let invalid = CString::new(vec![0xff]).unwrap();
        assert_eq!(
            session.rename(&invalid).unwrap_err().as_bytes(),
            b"invalid session name: \xff"
        );
        assert_eq!(session.name().as_bytes(), b"");
    }

    #[test]
    fn status_layout_is_cached_until_the_sizing_pass_publishes_it() {
        let session = session::new();
        let mut options = crate::options::options_create(None);
        for key in [c"status", c"status-position"] {
            let definition = crate::options::options_table
                .iter()
                .find(|definition| definition.name == Some(key))
                .unwrap();
            crate::options::options_default(&mut options, definition);
        }
        session.borrow_mut().options = Some(options);
        status_update_cache(&session);
        let before = session.status_layout();
        {
            let mut options = session.borrow_options_mut();
            crate::options::options_set_number(&mut options, c"status", 0);
        }
        assert_eq!(
            session.status_layout(),
            before,
            "configuration edits do not publish the sizing cache"
        );
        status_update_cache(&session);
        assert_eq!(session.status_layout(), (-1, 0));
    }
}
