use crate::src::cmd::parse::cmd_parse_from_string;
use crate::src::cmd::queue::{
    cmdq_append, cmdq_error, cmdq_get_callback_owned, cmdq_get_command, cmdq_insert_after,
    cmdq_new_state,
};
use crate::src::cmd::{cmd_list_all_have, cmd_list_print_cstring};
use crate::src::key_string::key_string_format;
use crate::src::log::{fatalx, log_cstr, log_debug, log_hex};
use crate::src::server_client::ClientRegistry;

use crate::src::server_client::Client as _;
use crate::src::shared::abi::*;
use crate::src::shared::client::ClientRef;
use crate::src::shared::client::CLIENT_READONLY;
use crate::src::shared::command::CMDQ_STATE_REPEAT;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd_find_state, cmd_list, cmdq_item};
use crate::src::shared::command::{cmd_parse_input, cmd_parse_result};
use crate::src::shared::key::KEY_BINDING_REPEAT;
use crate::src::shared::key::*;
use crate::src::shared::key::{key_binding, key_event, key_table};
use std::ffi::CStr;

/// Key tables indexed by name.
type KeyTableMap =
    std::collections::BTreeMap<std::ffi::CString, std::rc::Rc<std::cell::RefCell<key_table>>>;

#[repr(C)]
pub struct key_tables {
    pub(crate) storage: KeyTableMap,
}

thread_local! {
    static KEY_TABLES: std::cell::RefCell<key_tables> = const {
        std::cell::RefCell::new(key_tables {
            storage: KeyTableMap::new(),
        })
    };
}

/// The key table index owns the tables. Each call borrows it only for its own
/// duration and returns owned `Rc`s; removed tables are dropped by the caller.
impl key_tables {
    pub fn find(name: &CStr) -> Option<std::rc::Rc<std::cell::RefCell<key_table>>> {
        KEY_TABLES.with_borrow(|index| index.storage.get(name).cloned())
    }

    /// An existing table with the same name wins and is returned.
    pub fn insert(
        table: std::rc::Rc<std::cell::RefCell<key_table>>,
    ) -> Option<std::rc::Rc<std::cell::RefCell<key_table>>> {
        let key = table.borrow().name.clone();
        KEY_TABLES.with_borrow_mut(|index| match index.storage.entry(key) {
            std::collections::btree_map::Entry::Occupied(entry) => Some(entry.get().clone()),
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(table);
                None
            }
        })
    }

    /// Removes and returns the indexed owner only if it is `table`.
    pub fn remove(
        table: &std::rc::Rc<std::cell::RefCell<key_table>>,
    ) -> Option<std::rc::Rc<std::cell::RefCell<key_table>>> {
        let key = table.borrow().name.clone();
        KEY_TABLES.with_borrow_mut(|index| {
            if !index
                .storage
                .get(&key)
                .is_some_and(|candidate| std::rc::Rc::ptr_eq(candidate, table))
            {
                return None;
            }
            index.storage.remove(&key)
        })
    }

    /// Retain the tables, in name order, while callers borrow their bindings.
    pub fn snapshot() -> Vec<std::rc::Rc<std::cell::RefCell<key_table>>> {
        KEY_TABLES.with_borrow(|index| index.storage.values().cloned().collect())
    }
}

pub(crate) fn key_bindings_set_note(bd: &mut key_binding, note: Option<&CStr>) {
    bd.note = note.map(CStr::to_owned);
}

pub unsafe fn key_bindings_get_table(
    name: &CStr,
    create: i32,
) -> Option<std::rc::Rc<std::cell::RefCell<key_table>>> {
    if let Some(table) = key_tables::find(name) {
        return Some(table);
    }
    if create == 0 {
        return None;
    }
    let mut value = key_table::empty();
    value.name = name.to_owned();
    let table = std::rc::Rc::new(std::cell::RefCell::new(value));
    key_tables::insert(table.clone());
    Some(table)
}
pub fn key_bindings_get(table: &key_table, key: key_code) -> Option<&key_binding> {
    table.key_bindings.get(key)
}

pub fn key_bindings_get_default(table: &key_table, key: key_code) -> Option<&key_binding> {
    table.default_key_bindings.get(key)
}

pub unsafe fn key_bindings_add(
    name: &CStr,
    key: key_code,
    note: Option<&CStr>,
    repeat: ::core::ffi::c_int,
    cmdlist: Option<std::rc::Rc<std::cell::RefCell<cmd_list>>>,
) {
    let owner = key_bindings_get_table(name, 1).expect("created key table");
    let mut table = owner.borrow_mut();
    let key = key & !KEYC_MASK_FLAGS;
    let Some(cmdlist) = cmdlist else {
        if let Some(bd) = table.key_bindings.get_mut(key) {
            if let Some(note) = note {
                key_bindings_set_note(bd, Some(note));
            }
            if repeat != 0 {
                bd.flags |= KEY_BINDING_REPEAT;
            }
        }
        return;
    };
    drop(table.key_bindings.remove(key));
    let bd = Box::new(key_binding {
        key,
        commands: cmdlist,
        note: note.map(CStr::to_owned),
        tablename: Some(table.name.clone()),
        flags: if repeat != 0 { KEY_BINDING_REPEAT } else { 0 },
    });
    let s = cmd_list_print_cstring(&bd.cmdlist().borrow(), 0);
    let key_string = key_string_format(key, true);
    table.key_bindings.insert(bd);
    log_debug(format_args!(
        "key_bindings_add: {} {} = {}",
        log_hex(key),
        crate::src::log::log_bytes(key_string.as_bytes()),
        crate::src::log::log_bytes(s.as_bytes())
    ));
}

pub unsafe fn key_bindings_remove(name: &CStr, key: key_code) {
    let Some(owner) = key_bindings_get_table(name, 0) else {
        return;
    };
    let mut table = owner.borrow_mut();
    let Some(bd) = table.key_bindings.remove(key & !KEYC_MASK_FLAGS) else {
        return;
    };
    let key_string = key_string_format(bd.key, true);
    log_debug(format_args!(
        "key_bindings_remove: {} {}",
        log_hex(bd.key),
        crate::src::log::log_bytes(key_string.as_bytes())
    ));
    drop(bd);
    if table.key_bindings.storage.is_empty() && table.default_key_bindings.storage.is_empty() {
        drop(table);
        drop(key_tables::remove(&owner));
    }
}

pub unsafe fn key_bindings_reset(name: &CStr, key: key_code) {
    let Some(owner) = key_bindings_get_table(name, 0) else {
        return;
    };
    let mut table = owner.borrow_mut();
    let key = key & !KEYC_MASK_FLAGS;
    let table_ref = &mut *table;
    let Some(bd) = table_ref.key_bindings.get_mut(key) else {
        return;
    };
    let Some(dd) = table_ref.default_key_bindings.get(key) else {
        drop(table);
        key_bindings_remove(name, key);
        return;
    };
    bd.commands = std::rc::Rc::clone(&dd.commands);
    bd.note = dd.note.clone();
    bd.flags = dd.flags;
}

pub unsafe fn key_bindings_remove_table(name: &CStr) {
    let mut c: Option<ClientRef> = None;
    if let Some(owner) = key_bindings_get_table(name, 0) {
        let detached = key_tables::remove(&owner);
        let mut registry_c_owner = ClientRegistry::first();
        c = registry_c_owner.clone();
        while !c.is_none() {
            if c.as_ref().expect("live client").uses_key_table(&owner) {
                c.clone().expect("live client").set_key_table(None);
            }
            registry_c_owner =
                ClientRegistry::next(registry_c_owner.as_ref().expect("current registry client"));
            c = registry_c_owner.clone();
        }
        drop(detached);
    }
}

/// Snapshot startup defaults with their own command-list reference and note.
unsafe fn key_bindings_init_done() -> cmd_retval {
    for owner in key_tables::snapshot() {
        let mut table = owner.borrow_mut();
        let table_ref = &mut *table;
        for bd in table_ref.key_bindings.iter() {
            table_ref.default_key_bindings.insert(Box::new(key_binding {
                key: bd.key,
                commands: std::rc::Rc::clone(&bd.commands),
                note: bd.note.clone(),
                tablename: None,
                flags: bd.flags,
            }));
        }
    }
    CMD_RETURN_NORMAL
}

pub unsafe fn key_bindings_init() {
    static defaults: [&CStr; 292] = [
        c"bind -N 'Send the prefix key' C-b { send-prefix }",
        c"bind -N 'Rotate through the panes' C-o { rotate-window }",
        c"bind -N 'Suspend the current client' C-z { suspend-client }",
        c"bind -N 'Break pane to a new window' ! { break-pane }",
        c"bind -N 'Split window vertically' '\"' { split-window }",
        c"bind -N 'List all paste buffers' '#' { list-buffers }",
        c"bind -N 'Rename current session' '$' { command-prompt -I'#S' { rename-session -- '%%' } }",
        c"bind -N 'Split window horizontally' % { split-window -h }",
        c"bind -N 'Kill current window' & { confirm-before -p\"kill-window #W? (y/n)\" kill-window }",
        c"bind -N 'Prompt for window index to select' \"'\" { command-prompt -pindex { select-window -t ':%%' } }",
        c"bind -N 'Switch to previous client' ( { switch-client -p }",
        c"bind -N 'Switch to next client' ) { switch-client -n }",
        c"bind -N 'Rename current window' , { command-prompt -I'#W' { rename-window -- '%%' } }",
        c"bind -N 'Delete the most recent paste buffer' - { delete-buffer }",
        c"bind -N 'Move the current window' . { command-prompt { move-window -t '%%' } }",
        c"bind -N 'Describe key binding' '/' { command-prompt -kpkey  { list-keys -1N '%%' } }",
        c"bind -N 'Select window 0' 0 { select-window -t:=0 }",
        c"bind -N 'Select window 1' 1 { select-window -t:=1 }",
        c"bind -N 'Select window 2' 2 { select-window -t:=2 }",
        c"bind -N 'Select window 3' 3 { select-window -t:=3 }",
        c"bind -N 'Select window 4' 4 { select-window -t:=4 }",
        c"bind -N 'Select window 5' 5 { select-window -t:=5 }",
        c"bind -N 'Select window 6' 6 { select-window -t:=6 }",
        c"bind -N 'Select window 7' 7 { select-window -t:=7 }",
        c"bind -N 'Select window 8' 8 { select-window -t:=8 }",
        c"bind -N 'Select window 9' 9 { select-window -t:=9 }",
        c"bind -N 'Prompt for a command' : { command-prompt }",
        c"bind -N 'Move to the previously active pane' \\; { last-pane }",
        c"bind -N 'Choose a paste buffer from a list' = { choose-buffer }",
        c"bind -N 'List key bindings' ? { list-keys -N }",
        c"bind -N 'Choose and detach a client from a list' D { choose-client }",
        c"bind -N 'Switch to the last client' L { switch-client -l }",
        c"bind -N 'Clear the marked pane' M { select-pane -M }",
        c"bind -N 'Change the pane title' T { command-prompt -I'#T' { select-pane -T '%%' } }",
        c"bind -N 'Enter copy mode' [ { copy-mode }",
        c"bind -N 'Paste the most recent paste buffer' ] { paste-buffer -p }",
        c"bind -N 'Create a new window' c { new-window }",
        c"bind -N 'Detach the current client' d { detach-client }",
        c"bind -N 'Search for a pane' f { command-prompt { find-window -- '%%' } }",
        c"bind -N 'Display window information' i { display-message }",
        c"bind -N 'Select the previously current window' l { last-window }",
        c"bind -N 'Toggle the marked pane' m { select-pane -m }",
        c"bind -N 'Select the next window' n { next-window }",
        c"bind -N 'Select the next pane' o { select-pane -t:.+ }",
        c"bind -N 'Customize options' C { customize-mode }",
        c"bind -N 'Select the previous window' p { previous-window }",
        c"bind -N 'Display pane numbers' q { display-panes }",
        c"bind -N 'Redraw the current client' r { refresh-client }",
        c"bind -N 'Choose a session from a list' s { choose-tree -s }",
        c"bind -N 'Show a clock' t { clock-mode }",
        c"bind -N 'Switch to a window' Tab { new-pane -E; switch-mode -wk }",
        c"bind -N 'Switch to a session' BTab { new-pane -E; switch-mode -sk }",
        c"bind -N 'Choose a window from a list' w { choose-tree -w }",
        c"bind -N 'Kill the active pane' x { confirm-before -p\"kill-pane #P? (y/n)\" kill-pane }",
        c"bind -N 'Swap the active pane with the pane above' '{' { swap-pane -U }",
        c"bind -N 'Swap the active pane with the pane below' '}' { swap-pane -D }",
        c"bind -N 'Show messages' '~' { show-messages }",
        c"bind -N 'Enter copy mode and scroll up' PPage { copy-mode -u }",
        c"bind -N 'Select the pane to the left of the active pane' -r Left { select-pane -L }",
        c"bind -N 'Select the pane to the right of the active pane' -r Right { select-pane -R }",
        c"bind -N 'Select the pane above the active pane' -r Up { select-pane -U }",
        c"bind -N 'Select the pane below the active pane' -r Down { select-pane -D }",
        c"bind -N 'Arrange the window in the next layout' Space { select-layout -n }",
        c"bind -N 'Select the next window with an alert' M-n { next-window -a }",
        c"bind -N 'Rotate through the panes in reverse' M-o { rotate-window -D }",
        c"bind -N 'Select the previous window with an alert' M-p { previous-window -a }",
        c"bind -N 'Move the visible part of the window up' -r S-Up { refresh-client -U 10 }",
        c"bind -N 'Move the visible part of the window down' -r S-Down { refresh-client -D 10 }",
        c"bind -N 'Move the visible part of the window left' -r S-Left { refresh-client -L 10 }",
        c"bind -N 'Move the visible part of the window right' -r S-Right { refresh-client -R 10 }",
        c"bind -N 'Reset so the visible part of the window follows the cursor' -r DC { refresh-client -c }",
        c"bind -N 'Enter the strip key table' -n C-a { switch-client -Tstrip }",
        c"bind -N 'Insert a pane after the active pane in its directory' -Tstrip c { new-pane -c '#{pane_current_path}' }",
        c"bind -N 'Create a new window' -Tstrip C { new-window }",
        c"bind -N 'Select the pane to the left of the active pane' -Tstrip h { select-pane -L }",
        c"bind -N 'Select the pane to the right of the active pane' -Tstrip l { select-pane -R }",
        c"bind -N 'Select the pane above the active pane' -Tstrip k { select-pane -U }",
        c"bind -N 'Select the pane below the active pane' -Tstrip j { select-pane -D }",
        c"bind -N 'Arrange the window in the next layout' -Tstrip Space { select-layout -n }",
        c"bind -N 'Toggle the active pane between half and full width' -Tstrip f { resize-pane -Z }",
        c"bind -N 'Swap the active pane with the pane to the left' -Tstrip H { swap-pane -U }",
        c"bind -N 'Swap the active pane with the pane to the right' -Tstrip L { swap-pane -D }",
        c"bind -N 'Kill the active pane' -Tstrip x { confirm-before -p\"kill-pane #P? (y/n)\" kill-pane }",
        c"bind -N 'Select window 0' -Tstrip 0 { select-window -t:=0 }",
        c"bind -N 'Select window 1' -Tstrip 1 { select-window -t:=1 }",
        c"bind -N 'Select window 2' -Tstrip 2 { select-window -t:=2 }",
        c"bind -N 'Select window 3' -Tstrip 3 { select-window -t:=3 }",
        c"bind -N 'Select window 4' -Tstrip 4 { select-window -t:=4 }",
        c"bind -N 'Select window 5' -Tstrip 5 { select-window -t:=5 }",
        c"bind -N 'Select window 6' -Tstrip 6 { select-window -t:=6 }",
        c"bind -N 'Select window 7' -Tstrip 7 { select-window -t:=7 }",
        c"bind -N 'Select window 8' -Tstrip 8 { select-window -t:=8 }",
        c"bind -N 'Select window 9' -Tstrip 9 { select-window -t:=9 }",
        c"bind -N 'Detach the current client' -Tstrip d { detach-client }",
        c"bind -N 'Enter copy mode and scroll up' -Tstrip PPage { copy-mode -u }",
        c"bind -N 'Enter copy mode and scroll down' -Tstrip NPage { copy-mode -d }",
        c"bind -N 'Select the pane to the left of the active pane' -Tstrip -r Left { select-pane -L }",
        c"bind -N 'Select the pane to the right of the active pane' -Tstrip -r Right { select-pane -R }",
        c"bind -N 'Select the pane above the active pane' -Tstrip -r Up { select-pane -U }",
        c"bind -N 'Select the pane below the active pane' -Tstrip -r Down { select-pane -D }",
        c"bind -N 'Send C-a' -Tstrip C-a { send-keys C-a }",
        c"bind -N 'Display window menu' < { display-menu -xW -yW -T '#[align=centre]#{window_index}:#{window_name}'  '#{?#{>:#{session_windows},1},,-}Swap Left' 'l' {swap-window -t:-1} '#{?#{>:#{session_windows},1},,-}Swap Right' 'r' {swap-window -t:+1} '#{?pane_marked_set,,-}Swap Marked' 's' {swap-window} '' 'Kill' 'X' {kill-window} 'Respawn' 'R' {respawn-window -k} '#{?pane_marked,Unmark,Mark}' 'm' {select-pane -m} 'Rename' 'n' {command-prompt -FI \"#W\" {rename-window -t '#{window_id}' -- '%%'}} '' 'New After' 'w' {new-window -a} 'New At End' 'W' {new-window} }",
        c"bind -N 'Display pane menu' > { display-menu -xP -yP -T '#[align=centre]#{pane_index} (#{pane_id})'  '#{?#{m/r:(copy|view)-mode,#{pane_mode}},Go To Top,}' '<' {send -X history-top} '#{?#{m/r:(copy|view)-mode,#{pane_mode}},Go To Bottom,}' '>' {send -X history-bottom} '' '#{?#{==:#{pane_mode},copy-mode},#{?copy_line_numbers,Hide Line Numbers,Show Line Numbers},}' 'L' {send -X line-numbers-toggle} '#{?#{==:#{pane_mode},copy-mode},#{?refresh_active,Refresh Off,Refresh On},}' 'r' {send -X refresh-toggle} '' '#{?#{&&:#{buffer_size},#{!:#{pane_in_mode}}},Paste #[underscore]#{=/9/...:buffer_sample},}' 'p' {paste-buffer} '' '#{?mouse_word,Search For #[underscore]#{=/9/...:mouse_word},}' 'C-r' {if -F '#{?#{m/r:(copy|view)-mode,#{pane_mode}},0,1}' 'copy-mode -t='; send -Xt= search-backward -- \"#{q:mouse_word}\"} '#{?mouse_word,Type #[underscore]#{=/9/...:mouse_word},}' 'C-y' {copy-mode -q; send-keys -l -- \"#{q:mouse_word}\"} '#{?mouse_word,Copy #[underscore]#{=/9/...:mouse_word},}' 'c' {copy-mode -q; set-buffer -- \"#{q:mouse_word}\"} '#{?mouse_line,Copy Line,}' 'l' {copy-mode -q; set-buffer -- \"#{q:mouse_line}\"} '' '#{?mouse_hyperlink,Type #[underscore]#{=/9/...:mouse_hyperlink},}' 'C-h' {copy-mode -q; send-keys -l -- \"#{q:mouse_hyperlink}\"} '#{?mouse_hyperlink,Copy #[underscore]#{=/9/...:mouse_hyperlink},}' 'h' {copy-mode -q; set-buffer -- \"#{q:mouse_hyperlink}\"} '' 'Horizontal Split' 'h' {split-window -h} 'Vertical Split' 'v' {split-window -v} '' '#{?#{>:#{window_panes},1},,-}Swap Up' 'u' {swap-pane -U} '#{?#{>:#{window_panes},1},,-}Swap Down' 'd' {swap-pane -D} '#{?pane_marked_set,,-}Swap Marked' 's' {swap-pane} '' 'Kill' 'X' {kill-pane} 'Respawn' 'R' {respawn-pane -k} '#{?pane_marked,Unmark,Mark}' 'm' {select-pane -m} }",
        c"bind -n MouseDown1Pane { select-pane -t=; send -M }",
        c"bind -n MouseDrag1Pane { if -F '#{||:#{pane_in_mode},#{mouse_any_flag}}' { send -M } { copy-mode -M } }",
        c"bind -n WheelUpPane { if -F '#{||:#{alternate_on},#{pane_in_mode},#{mouse_any_flag}}' { send -M } { copy-mode -e } }",
        c"bind -n MouseDown2Pane { select-pane -t=; if -F '#{||:#{pane_in_mode},#{mouse_any_flag}}' { send -M } { paste -p } }",
        c"bind -n DoubleClick1Pane { select-pane -t=; if -F '#{||:#{pane_in_mode},#{mouse_any_flag}}' { send -M } { copy-mode -H; send -X select-word; run -d0.3; send -X copy-pipe-and-cancel } }",
        c"bind -n TripleClick1Pane { select-pane -t=; if -F '#{||:#{pane_in_mode},#{mouse_any_flag}}' { send -M } { copy-mode -H; send -X select-line; run -d0.3; send -X copy-pipe-and-cancel } }",
        c"bind -n MouseDown1Border { select-pane -M }",
        c"bind -n MouseDown1Status { switch-client -t= }",
        c"bind -n C-MouseDown1Status { swap-window -t@ }",
        c"bind -n MouseDown1Control9 { display-menu -t= -xM -yM -O -T 'Kill pane #{pane_index}?' 'Yes' 'y' { kill-pane -t= } 'No' 'n' {}}",
        c"bind -n WheelDownStatus { next-window }",
        c"bind -n WheelUpStatus { previous-window }",
        c"bind -n MouseDown3StatusLeft { run -C \"display-menu -t= -xM -yW -T '#[align=centre]#{session_name}'  #{S/t:#{?#{&&:#{<:#{loop_index},6},#{!:#{session_active}}},'Switch To #[underscore]#{session_name}' '' {switch-client -t=#{session_id}#} ,}} '' 'Renumber' 'N' {move-window -r} 'Rename' 'r' {command-prompt -I '#S' {rename-session -- '%%'}} 'Detach' 'd' {detach-client} '' 'New Session' 's' {new-session} 'New Window' 'w' {new-window}\" }",
        c"bind -n M-MouseDown3StatusLeft { run -C \"display-menu -t= -xM -yW -T '#[align=centre]#{session_name}'  #{S/t:#{?#{&&:#{<:#{loop_index},6},#{!:#{session_active}}},'Switch To #[underscore]#{session_name}' '' {switch-client -t=#{session_id}#} ,}} '' 'Renumber' 'N' {move-window -r} 'Rename' 'r' {command-prompt -I '#S' {rename-session -- '%%'}} 'Detach' 'd' {detach-client} '' 'New Session' 's' {new-session} 'New Window' 'w' {new-window}\" }",
        c"bind -n MouseDown3Status { display-menu -t= -xW -yW -T '#[align=centre]#{window_index}:#{window_name}'  '#{?#{>:#{session_windows},1},,-}Swap Left' 'l' {swap-window -t:-1} '#{?#{>:#{session_windows},1},,-}Swap Right' 'r' {swap-window -t:+1} '#{?pane_marked_set,,-}Swap Marked' 's' {swap-window} '' 'Kill' 'X' {kill-window} 'Respawn' 'R' {respawn-window -k} '#{?pane_marked,Unmark,Mark}' 'm' {select-pane -m} 'Rename' 'n' {command-prompt -FI \"#W\" {rename-window -t '#{window_id}' -- '%%'}} '' 'New After' 'w' {new-window -a} 'New At End' 'W' {new-window}}",
        c"bind -n M-MouseDown3Status { display-menu -t= -xW -yW -T '#[align=centre]#{window_index}:#{window_name}'  '#{?#{>:#{session_windows},1},,-}Swap Left' 'l' {swap-window -t:-1} '#{?#{>:#{session_windows},1},,-}Swap Right' 'r' {swap-window -t:+1} '#{?pane_marked_set,,-}Swap Marked' 's' {swap-window} '' 'Kill' 'X' {kill-window} 'Respawn' 'R' {respawn-window -k} '#{?pane_marked,Unmark,Mark}' 'm' {select-pane -m} 'Rename' 'n' {command-prompt -FI \"#W\" {rename-window -t '#{window_id}' -- '%%'}} '' 'New After' 'w' {new-window -a} 'New At End' 'W' {new-window}}",
        c"bind -n MouseDown3Pane { if -Ft= '#{||:#{mouse_any_flag},#{&&:#{pane_in_mode},#{?#{m/r:(copy|view)-mode,#{pane_mode}},0,1}}}' { select-pane -t=; send -M } { display-menu -t= -xM -yM -T '#[align=centre]#{pane_index} (#{pane_id})'  '#{?#{m/r:(copy|view)-mode,#{pane_mode}},Go To Top,}' '<' {send -X history-top} '#{?#{m/r:(copy|view)-mode,#{pane_mode}},Go To Bottom,}' '>' {send -X history-bottom} '' '#{?#{==:#{pane_mode},copy-mode},#{?copy_line_numbers,Hide Line Numbers,Show Line Numbers},}' 'L' {send -X line-numbers-toggle} '#{?#{==:#{pane_mode},copy-mode},#{?refresh_active,Refresh Off,Refresh On},}' 'r' {send -X refresh-toggle} '' '#{?#{&&:#{buffer_size},#{!:#{pane_in_mode}}},Paste #[underscore]#{=/9/...:buffer_sample},}' 'p' {paste-buffer} '' '#{?mouse_word,Search For #[underscore]#{=/9/...:mouse_word},}' 'C-r' {if -F '#{?#{m/r:(copy|view)-mode,#{pane_mode}},0,1}' 'copy-mode -t='; send -Xt= search-backward -- \"#{q:mouse_word}\"} '#{?mouse_word,Type #[underscore]#{=/9/...:mouse_word},}' 'C-y' {copy-mode -q; send-keys -l -- \"#{q:mouse_word}\"} '#{?mouse_word,Copy #[underscore]#{=/9/...:mouse_word},}' 'c' {copy-mode -q; set-buffer -- \"#{q:mouse_word}\"} '#{?mouse_line,Copy Line,}' 'l' {copy-mode -q; set-buffer -- \"#{q:mouse_line}\"} '' '#{?mouse_hyperlink,Type #[underscore]#{=/9/...:mouse_hyperlink},}' 'C-h' {copy-mode -q; send-keys -l -- \"#{q:mouse_hyperlink}\"} '#{?mouse_hyperlink,Copy #[underscore]#{=/9/...:mouse_hyperlink},}' 'h' {copy-mode -q; set-buffer -- \"#{q:mouse_hyperlink}\"} '' 'Horizontal Split' 'h' {split-window -h} 'Vertical Split' 'v' {split-window -v} '' '#{?#{>:#{window_panes},1},,-}Swap Up' 'u' {swap-pane -U} '#{?#{>:#{window_panes},1},,-}Swap Down' 'd' {swap-pane -D} '#{?pane_marked_set,,-}Swap Marked' 's' {swap-pane} '' 'Kill' 'X' {kill-pane} 'Respawn' 'R' {respawn-pane -k} '#{?pane_marked,Unmark,Mark}' 'm' {select-pane -m} } }",
        c"bind -n M-MouseDown3Pane { display-menu -t= -xM -yM -T '#[align=centre]#{pane_index} (#{pane_id})'  '#{?#{m/r:(copy|view)-mode,#{pane_mode}},Go To Top,}' '<' {send -X history-top} '#{?#{m/r:(copy|view)-mode,#{pane_mode}},Go To Bottom,}' '>' {send -X history-bottom} '' '#{?#{==:#{pane_mode},copy-mode},#{?copy_line_numbers,Hide Line Numbers,Show Line Numbers},}' 'L' {send -X line-numbers-toggle} '#{?#{==:#{pane_mode},copy-mode},#{?refresh_active,Refresh Off,Refresh On},}' 'r' {send -X refresh-toggle} '' '#{?#{&&:#{buffer_size},#{!:#{pane_in_mode}}},Paste #[underscore]#{=/9/...:buffer_sample},}' 'p' {paste-buffer} '' '#{?mouse_word,Search For #[underscore]#{=/9/...:mouse_word},}' 'C-r' {if -F '#{?#{m/r:(copy|view)-mode,#{pane_mode}},0,1}' 'copy-mode -t='; send -Xt= search-backward -- \"#{q:mouse_word}\"} '#{?mouse_word,Type #[underscore]#{=/9/...:mouse_word},}' 'C-y' {copy-mode -q; send-keys -l -- \"#{q:mouse_word}\"} '#{?mouse_word,Copy #[underscore]#{=/9/...:mouse_word},}' 'c' {copy-mode -q; set-buffer -- \"#{q:mouse_word}\"} '#{?mouse_line,Copy Line,}' 'l' {copy-mode -q; set-buffer -- \"#{q:mouse_line}\"} '' '#{?mouse_hyperlink,Type #[underscore]#{=/9/...:mouse_hyperlink},}' 'C-h' {copy-mode -q; send-keys -l -- \"#{q:mouse_hyperlink}\"} '#{?mouse_hyperlink,Copy #[underscore]#{=/9/...:mouse_hyperlink},}' 'h' {copy-mode -q; set-buffer -- \"#{q:mouse_hyperlink}\"} '' 'Horizontal Split' 'h' {split-window -h} 'Vertical Split' 'v' {split-window -v} '' '#{?#{>:#{window_panes},1},,-}Swap Up' 'u' {swap-pane -U} '#{?#{>:#{window_panes},1},,-}Swap Down' 'd' {swap-pane -D} '#{?pane_marked_set,,-}Swap Marked' 's' {swap-pane} '' 'Kill' 'X' {kill-pane} 'Respawn' 'R' {respawn-pane -k} '#{?pane_marked,Unmark,Mark}' 'm' {select-pane -m} }",
        c"bind -n MouseDown3Empty { display-menu -t= -xM -yM -T '#[align=centre]#{window_index}:#{window_name}'  'New Pane' 'p' {new-pane} 'New Window' 'w' {new-window} }",
        c"bind -n M-MouseDown3Empty { display-menu -t= -xM -yM -T '#[align=centre]#{window_index}:#{window_name}'  'New Pane' 'p' {new-pane} 'New Window' 'w' {new-window} }",
        c"bind -n MouseDown1ScrollbarUp { if -Ft= '#{pane_in_mode}' { send -X page-up } {copy-mode -u } }",
        c"bind -n MouseDown1ScrollbarDown { if -Ft= '#{pane_in_mode}' { send -X page-down } {copy-mode -d } }",
        c"bind -n MouseDrag1ScrollbarSlider { if -Ft= '#{pane_in_mode}' { send -X scroll-to-mouse } { copy-mode -S } }",
        c"bind -Tcopy-mode C-Space { send -X begin-selection }",
        c"bind -Tcopy-mode C-a { send -X start-of-line }",
        c"bind -Tcopy-mode C-c { send -X cancel }",
        c"bind -Tcopy-mode C-e { send -X end-of-line }",
        c"bind -Tcopy-mode C-f { send -X cursor-right }",
        c"bind -Tcopy-mode C-b { send -X cursor-left }",
        c"bind -Tcopy-mode C-g { send -X clear-selection }",
        c"bind -Tcopy-mode C-k { send -X copy-pipe-end-of-line-and-cancel }",
        c"bind -Tcopy-mode C-l { send -X recentre-top-bottom }",
        c"bind -Tcopy-mode M-l { send -X cursor-centre-horizontal }",
        c"bind -Tcopy-mode C-n { send -X cursor-down }",
        c"bind -Tcopy-mode C-p { send -X cursor-up }",
        c"bind -Tcopy-mode C-r { command-prompt -P -T search -ip'(search up)' -I'#{pane_search_string}' { send -X search-backward-incremental -- '%%' } }",
        c"bind -Tcopy-mode C-s { command-prompt -P -T search -ip'(search down)' -I'#{pane_search_string}' { send -X search-forward-incremental -- '%%' } }",
        c"bind -Tcopy-mode C-v { send -X page-down }",
        c"bind -Tcopy-mode C-w { send -X copy-pipe-and-cancel }",
        c"bind -Tcopy-mode Escape { send -X cancel }",
        c"bind -Tcopy-mode C-[ { send -X cancel }",
        c"bind -Tcopy-mode Space { send -X page-down }",
        c"bind -Tcopy-mode , { send -X jump-reverse }",
        c"bind -Tcopy-mode \\; { send -X jump-again }",
        c"bind -Tcopy-mode F { command-prompt -P -1p'(jump backward)' { send -X jump-backward -- '%%' } }",
        c"bind -Tcopy-mode L { send -X line-numbers-toggle }",
        c"bind -Tcopy-mode N { send -X search-reverse }",
        c"bind -Tcopy-mode P { send -X toggle-position }",
        c"bind -Tcopy-mode R { send -X rectangle-toggle }",
        c"bind -Tcopy-mode T { command-prompt -P -1p'(jump to backward)' { send -X jump-to-backward -- '%%' } }",
        c"bind -Tcopy-mode X { send -X set-mark }",
        c"bind -Tcopy-mode f { command-prompt -P -1p'(jump forward)' { send -X jump-forward -- '%%' } }",
        c"bind -Tcopy-mode g { command-prompt -P -p'(goto line)' { send -X goto-line -- '%%' } }",
        c"bind -Tcopy-mode n { send -X search-again }",
        c"bind -Tcopy-mode q { send -X cancel }",
        c"bind -Tcopy-mode r { send -X refresh-now }",
        c"bind -Tcopy-mode t { command-prompt -P -1p'(jump to forward)' { send -X jump-to-forward -- '%%' } }",
        c"bind -Tcopy-mode Home { send -X start-of-line }",
        c"bind -Tcopy-mode End { send -X end-of-line }",
        c"bind -Tcopy-mode MouseDown1Pane select-pane",
        c"bind -Tcopy-mode MouseDrag1Pane { select-pane; send -X begin-selection }",
        c"bind -Tcopy-mode MouseDragEnd1Pane { send -X copy-pipe-and-cancel }",
        c"bind -Tcopy-mode WheelUpPane { select-pane; send -N5 -X scroll-up }",
        c"bind -Tcopy-mode WheelDownPane { select-pane; send -N5 -X scroll-down }",
        c"bind -Tcopy-mode DoubleClick1Pane { select-pane; send -X select-word; run -d0.3; send -X copy-pipe-and-cancel }",
        c"bind -Tcopy-mode TripleClick1Pane { select-pane; send -X select-line; run -d0.3; send -X copy-pipe-and-cancel }",
        c"bind -Tcopy-mode NPage { send -X page-down }",
        c"bind -Tcopy-mode PPage { send -X page-up }",
        c"bind -Tcopy-mode Up { send -X cursor-up }",
        c"bind -Tcopy-mode Down { send -X cursor-down }",
        c"bind -Tcopy-mode Left { send -X cursor-left }",
        c"bind -Tcopy-mode Right { send -X cursor-right }",
        c"bind -Tcopy-mode M-1 { command-prompt -P -Np'(repeat)' -I1 { send -N '%%' } }",
        c"bind -Tcopy-mode M-2 { command-prompt -P -Np'(repeat)' -I2 { send -N '%%' } }",
        c"bind -Tcopy-mode M-3 { command-prompt -P -Np'(repeat)' -I3 { send -N '%%' } }",
        c"bind -Tcopy-mode M-4 { command-prompt -P -Np'(repeat)' -I4 { send -N '%%' } }",
        c"bind -Tcopy-mode M-5 { command-prompt -P -Np'(repeat)' -I5 { send -N '%%' } }",
        c"bind -Tcopy-mode M-6 { command-prompt -P -Np'(repeat)' -I6 { send -N '%%' } }",
        c"bind -Tcopy-mode M-7 { command-prompt -P -Np'(repeat)' -I7 { send -N '%%' } }",
        c"bind -Tcopy-mode M-8 { command-prompt -P -Np'(repeat)' -I8 { send -N '%%' } }",
        c"bind -Tcopy-mode M-9 { command-prompt -P -Np'(repeat)' -I9 { send -N '%%' } }",
        c"bind -Tcopy-mode M-< { send -X history-top }",
        c"bind -Tcopy-mode M-> { send -X history-bottom }",
        c"bind -Tcopy-mode M-R { send -X top-line }",
        c"bind -Tcopy-mode M-b { send -X previous-word }",
        c"bind -Tcopy-mode C-M-b { send -X previous-matching-bracket }",
        c"bind -Tcopy-mode M-f { send -X next-word-end }",
        c"bind -Tcopy-mode C-M-f { send -X next-matching-bracket }",
        c"bind -Tcopy-mode M-m { send -X back-to-indentation }",
        c"bind -Tcopy-mode M-r { send -X middle-line }",
        c"bind -Tcopy-mode M-v { send -X page-up }",
        c"bind -Tcopy-mode M-w { send -X copy-pipe-and-cancel }",
        c"bind -Tcopy-mode M-x { send -X jump-to-mark }",
        c"bind -Tcopy-mode 'M-{' { send -X previous-paragraph }",
        c"bind -Tcopy-mode 'M-}' { send -X next-paragraph }",
        c"bind -Tcopy-mode M-Up { send -X halfpage-up }",
        c"bind -Tcopy-mode M-Down { send -X halfpage-down }",
        c"bind -Tcopy-mode C-Up { send -X scroll-up }",
        c"bind -Tcopy-mode C-Down { send -X scroll-down }",
        c"bind -Tcopy-mode M-C-Up { send -X previous-prompt }",
        c"bind -Tcopy-mode M-C-Down { send -X next-prompt }",
        c"bind -Tcopy-mode-vi '#' { send -FX search-backward -- '#{copy_cursor_word}' }",
        c"bind -Tcopy-mode-vi * { send -FX search-forward -- '#{copy_cursor_word}' }",
        c"bind -Tcopy-mode-vi C-c { send -X cancel }",
        c"bind -Tcopy-mode-vi C-d { send -X halfpage-down }",
        c"bind -Tcopy-mode-vi C-e { send -X scroll-down }",
        c"bind -Tcopy-mode-vi C-b { send -X page-up }",
        c"bind -Tcopy-mode-vi C-f { send -X page-down }",
        c"bind -Tcopy-mode-vi C-h { send -X cursor-left }",
        c"bind -Tcopy-mode-vi C-j { send -X copy-pipe-and-cancel }",
        c"bind -Tcopy-mode-vi Enter { send -X copy-pipe-and-cancel }",
        c"bind -Tcopy-mode-vi C-u { send -X halfpage-up }",
        c"bind -Tcopy-mode-vi C-v { send -X rectangle-toggle }",
        c"bind -Tcopy-mode-vi C-y { send -X scroll-up }",
        c"bind -Tcopy-mode-vi Escape { send -X clear-selection }",
        c"bind -Tcopy-mode-vi C-[ { send -X clear-selection }",
        c"bind -Tcopy-mode-vi Space { send -X begin-selection }",
        c"bind -Tcopy-mode-vi '$' { send -X end-of-line }",
        c"bind -Tcopy-mode-vi , { send -X jump-reverse }",
        c"bind -Tcopy-mode-vi / { command-prompt -P -T search -p'(search down)' { send -X search-forward -- '%%' } }",
        c"bind -Tcopy-mode-vi 0 { send -X start-of-line }",
        c"bind -Tcopy-mode-vi 1 { command-prompt -P -Np'(repeat)' -I1 { send -N '%%' } }",
        c"bind -Tcopy-mode-vi 2 { command-prompt -P -Np'(repeat)' -I2 { send -N '%%' } }",
        c"bind -Tcopy-mode-vi 3 { command-prompt -P -Np'(repeat)' -I3 { send -N '%%' } }",
        c"bind -Tcopy-mode-vi 4 { command-prompt -P -Np'(repeat)' -I4 { send -N '%%' } }",
        c"bind -Tcopy-mode-vi 5 { command-prompt -P -Np'(repeat)' -I5 { send -N '%%' } }",
        c"bind -Tcopy-mode-vi 6 { command-prompt -P -Np'(repeat)' -I6 { send -N '%%' } }",
        c"bind -Tcopy-mode-vi 7 { command-prompt -P -Np'(repeat)' -I7 { send -N '%%' } }",
        c"bind -Tcopy-mode-vi 8 { command-prompt -P -Np'(repeat)' -I8 { send -N '%%' } }",
        c"bind -Tcopy-mode-vi 9 { command-prompt -P -Np'(repeat)' -I9 { send -N '%%' } }",
        c"bind -Tcopy-mode-vi : { command-prompt -P -p'(goto line)' { send -X goto-line -- '%%' } }",
        c"bind -Tcopy-mode-vi \\; { send -X jump-again }",
        c"bind -Tcopy-mode-vi ? { command-prompt -P -T search -p'(search up)' { send -X search-backward -- '%%' } }",
        c"bind -Tcopy-mode-vi A { send -X append-selection-and-cancel }",
        c"bind -Tcopy-mode-vi B { send -X previous-space }",
        c"bind -Tcopy-mode-vi D { send -X copy-pipe-end-of-line-and-cancel }",
        c"bind -Tcopy-mode-vi E { send -X next-space-end }",
        c"bind -Tcopy-mode-vi F { command-prompt -P -1p'(jump backward)' { send -X jump-backward -- '%%' } }",
        c"bind -Tcopy-mode-vi G { send -X history-bottom }",
        c"bind -Tcopy-mode-vi H { send -X top-line }",
        c"bind -Tcopy-mode-vi J { send -X scroll-down }",
        c"bind -Tcopy-mode-vi K { send -X scroll-up }",
        c"bind -Tcopy-mode-vi L { send -X bottom-line }",
        c"bind -Tcopy-mode-vi M { send -X middle-line }",
        c"bind -Tcopy-mode-vi N { send -X search-reverse }",
        c"bind -Tcopy-mode-vi P { send -X toggle-position }",
        c"bind -Tcopy-mode-vi T { command-prompt -P -1p'(jump to backward)' { send -X jump-to-backward -- '%%' } }",
        c"bind -Tcopy-mode-vi V { send -X select-line }",
        c"bind -Tcopy-mode-vi W { send -X next-space }",
        c"bind -Tcopy-mode-vi X { send -X set-mark }",
        c"bind -Tcopy-mode-vi ^ { send -X back-to-indentation }",
        c"bind -Tcopy-mode-vi b { send -X previous-word }",
        c"bind -Tcopy-mode-vi e { send -X next-word-end }",
        c"bind -Tcopy-mode-vi f { command-prompt -P -1p'(jump forward)' { send -X jump-forward -- '%%' } }",
        c"bind -Tcopy-mode-vi g { send -X history-top }",
        c"bind -Tcopy-mode-vi h { send -X cursor-left }",
        c"bind -Tcopy-mode-vi j { send -X cursor-down }",
        c"bind -Tcopy-mode-vi k { send -X cursor-up }",
        c"bind -Tcopy-mode-vi z { send -X scroll-middle }",
        c"bind -Tcopy-mode-vi l { send -X cursor-right }",
        c"bind -Tcopy-mode-vi n { send -X search-again }",
        c"bind -Tcopy-mode-vi o { send -X other-end }",
        c"bind -Tcopy-mode-vi q { send -X cancel }",
        c"bind -Tcopy-mode-vi r { send -X refresh-now }",
        c"bind -Tcopy-mode-vi t { command-prompt -P -1p'(jump to forward)' { send -X jump-to-forward -- '%%' } }",
        c"bind -Tcopy-mode-vi v { send -X rectangle-toggle }",
        c"bind -Tcopy-mode-vi w { send -X next-word }",
        c"bind -Tcopy-mode-vi '{' { send -X previous-paragraph }",
        c"bind -Tcopy-mode-vi '}' { send -X next-paragraph }",
        c"bind -Tcopy-mode-vi % { send -X next-matching-bracket }",
        c"bind -Tcopy-mode-vi Home { send -X start-of-line }",
        c"bind -Tcopy-mode-vi End { send -X end-of-line }",
        c"bind -Tcopy-mode-vi MouseDown1Pane { select-pane }",
        c"bind -Tcopy-mode-vi MouseDrag1Pane { select-pane; send -X begin-selection }",
        c"bind -Tcopy-mode-vi MouseDragEnd1Pane { send -X copy-pipe-and-cancel }",
        c"bind -Tcopy-mode-vi WheelUpPane { select-pane; send -N5 -X scroll-up }",
        c"bind -Tcopy-mode-vi WheelDownPane { select-pane; send -N5 -X scroll-down }",
        c"bind -Tcopy-mode-vi DoubleClick1Pane { select-pane; send -X select-word; run -d0.3; send -X copy-pipe-and-cancel }",
        c"bind -Tcopy-mode-vi TripleClick1Pane { select-pane; send -X select-line; run -d0.3; send -X copy-pipe-and-cancel }",
        c"bind -Tcopy-mode-vi BSpace { send -X cursor-left }",
        c"bind -Tcopy-mode-vi NPage { send -X page-down }",
        c"bind -Tcopy-mode-vi PPage { send -X page-up }",
        c"bind -Tcopy-mode-vi Up { send -X cursor-up }",
        c"bind -Tcopy-mode-vi Down { send -X cursor-down }",
        c"bind -Tcopy-mode-vi Left { send -X cursor-left }",
        c"bind -Tcopy-mode-vi Right { send -X cursor-right }",
        c"bind -Tcopy-mode-vi M-x { send -X jump-to-mark }",
        c"bind -Tcopy-mode-vi C-Up { send -X scroll-up }",
        c"bind -Tcopy-mode-vi C-Down { send -X scroll-down }",
    ];
    let mut pr: cmd_parse_result = cmd_parse_result::empty();
    for command in defaults {
        pr = cmd_parse_from_string(command, ::core::ptr::null_mut::<cmd_parse_input>());
        if pr.status as ::core::ffi::c_uint
            != CMD_PARSE_SUCCESS as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            log_debug(format_args!(
                "{}",
                log_cstr(pr.error.as_deref().unwrap_or(c"(null)"))
            ));
            fatalx(|out| {
                out.write_all(b"bad default key: ")?;
                out.write_all(command.to_bytes())
            });
        }
        cmdq_append(
            None,
            cmdq_get_command(pr.cmdlist.as_ref().expect("successful command parse"), None),
        );
        drop(pr.cmdlist.take());
    }
    cmdq_append(
        None,
        cmdq_get_callback_owned(
            c"key_bindings_init_done",
            Some(Box::new(|_| unsafe { key_bindings_init_done() })),
        ),
    );
}
unsafe fn key_bindings_read_only(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    cmdq_error(item_handle, |out| out.write_all(b"client is read-only"));
    CMD_RETURN_ERROR
}
pub unsafe fn key_bindings_dispatch(
    bd: KeyBindingCommand,
    item_handle: Option<&std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>>,
    c_owner: Option<&ClientRef>,
    mut event: *mut key_event,
    mut fs: *mut cmd_find_state,
) -> std::rc::Weak<std::cell::UnsafeCell<cmdq_item>> {
    let mut c: Option<ClientRef> = c_owner.cloned();
    let new_item_allocation;
    let new_state;
    let mut readonly: ::core::ffi::c_int = 0;
    let mut flags: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if c.is_none() || !c.as_ref().expect("live client").flags() & CLIENT_READONLY as uint64_t != 0 {
        readonly = 1 as ::core::ffi::c_int;
    } else {
        readonly = cmd_list_all_have(&bd.cmdlist().borrow());
    }
    if readonly == 0 {
        new_item_allocation = cmdq_get_callback_owned(
            c"key_bindings_read_only",
            Some(Box::new(|item| unsafe { key_bindings_read_only(item) })),
        );
    } else {
        if bd.flags & KEY_BINDING_REPEAT != 0 {
            flags |= CMDQ_STATE_REPEAT;
        }
        new_state = cmdq_new_state(fs, event, flags);
        new_item_allocation = cmdq_get_command(&bd.commands, Some(&new_state));
    }
    if let Some(item) = item_handle {
        cmdq_insert_after(item, new_item_allocation)
    } else {
        cmdq_append(c_owner, new_item_allocation)
    }
}

pub fn key_bindings_has_repeat(bindings: &[&key_binding]) -> bool {
    bindings.iter().any(|bd| bd.flags & KEY_BINDING_REPEAT != 0)
}

#[cfg(test)]
mod ownership_tests {
    use super::*;
    use crate::src::cmd::cmd_list_new;

    use std::rc::Rc;

    unsafe fn binding(key: key_code) -> Box<key_binding> {
        Box::new(key_binding {
            key,
            commands: cmd_list_new(),
            note: None,
            tablename: None,
            flags: 0,
        })
    }

    #[test]
    fn table_index_returns_owners_and_removes_only_matching_allocations() {
        use std::cell::RefCell;
        let mut a = key_table::empty();
        a.name = c"a".to_owned();
        let a = Rc::new(RefCell::new(a));
        let mut b = key_table::empty();
        b.name = c"b".to_owned();
        let b = Rc::new(RefCell::new(b));
        let mut duplicate = key_table::empty();
        duplicate.name = c"a".to_owned();
        let duplicate = Rc::new(RefCell::new(duplicate));
        assert_ne!(a.borrow().identity, duplicate.borrow().identity);
        assert!(key_tables::insert(a.clone()).is_none());
        assert!(key_tables::insert(b.clone()).is_none());
        assert!(Rc::ptr_eq(
            &key_tables::insert(duplicate.clone()).unwrap(),
            &a
        ));
        assert!(key_tables::remove(&duplicate).is_none());
        assert!(Rc::ptr_eq(&key_tables::snapshot()[0], &a));
        let observed = Rc::downgrade(&a);
        let found = key_tables::find(c"a").unwrap();
        drop(key_tables::remove(&a));
        drop(a);
        assert!(observed.upgrade().is_some());
        drop(found);
        assert!(observed.upgrade().is_none());
        drop(key_tables::remove(&b));
        assert!(key_tables::snapshot().is_empty());
    }

    #[test]
    fn index_owns_bindings_and_keeps_addresses_stable() {
        unsafe {
            let mut index = key_bindings::default();
            let first = binding(8);
            let address = std::ptr::from_ref(&*first);
            let commands = Rc::downgrade(&first.commands);
            index.insert(first);
            index.insert(binding(2));
            index.insert(binding(13));
            assert_eq!(
                index.iter().map(|bd| bd.key).collect::<Vec<_>>(),
                [2, 8, 13]
            );
            assert_eq!(std::ptr::from_ref(index.get(8).unwrap()), address);
            let detached = index.remove(8).unwrap();
            assert_eq!(std::ptr::from_ref(&*detached), address);
            assert!(index.get(8).is_none());
            assert!(commands.upgrade().is_some());
            drop(detached);
            assert!(commands.upgrade().is_none());
            drop(index.remove(2));
            drop(index.remove(13));
            assert!(key_tables::snapshot().is_empty());
        }
    }

    #[test]
    fn defaults_restore_notes_flags_and_command_ownership() {
        unsafe {
            let name = c"binding-owner-defaults";
            let original = cmd_list_new();
            let original_lifetime = std::rc::Rc::downgrade(&original);
            key_bindings_add(name, 65, Some(c"original"), 1, Some(original));
            let table_owner = key_bindings_get_table(name, 0).unwrap();
            key_bindings_init_done();
            assert_eq!(original_lifetime.strong_count(), 2);
            let replacement = cmd_list_new();
            let replacement_lifetime = std::rc::Rc::downgrade(&replacement);
            key_bindings_add(name, 65, Some(c"replacement"), 0, Some(replacement));
            assert_eq!(original_lifetime.strong_count(), 1);
            key_bindings_reset(name, 65);
            assert!(replacement_lifetime.upgrade().is_none());
            let table_borrow = table_owner.borrow();
            let bd = key_bindings_get(&table_borrow, 65).unwrap();
            assert_eq!(bd.note.as_deref(), Some(c"original"));
            assert_eq!(bd.flags, KEY_BINDING_REPEAT);
            assert!(std::rc::Weak::ptr_eq(
                &std::rc::Rc::downgrade(bd.cmdlist()),
                &original_lifetime
            ));
            assert_eq!(original_lifetime.strong_count(), 2);
            drop(table_borrow);
            key_bindings_add(name, 65, Some(c"note only"), 0, None);
            assert!(std::rc::Weak::ptr_eq(
                &std::rc::Rc::downgrade(
                    key_bindings_get(&table_owner.borrow(), 65)
                        .unwrap()
                        .cmdlist()
                ),
                &original_lifetime
            ));
            assert_eq!(original_lifetime.strong_count(), 2);
            key_bindings_add(name, 66, None, 0, Some(cmd_list_new()));
            key_bindings_reset(name, 66);
            assert!(key_bindings_get(&table_owner.borrow(), 66).is_none());
            drop(table_owner);
            key_bindings_remove_table(name);
            assert!(original_lifetime.upgrade().is_none());
        }
    }

    #[test]
    fn dispatch_snapshot_survives_binding_replacement_and_table_removal() {
        unsafe {
            let name = c"binding-owner-dispatch";
            let original = cmd_list_new();
            let original_lifetime = std::rc::Rc::downgrade(&original);
            key_bindings_add(name, 65, None, 1, Some(original));
            let table_owner = key_bindings_get_table(name, 0).unwrap();
            let table_lifetime = Rc::downgrade(&table_owner);
            let retained_table = table_lifetime.upgrade().unwrap();
            let command = key_bindings_get(&table_owner.borrow(), 65)
                .unwrap()
                .command();
            key_bindings_add(name, 65, None, 0, Some(cmd_list_new()));
            assert!(std::rc::Weak::ptr_eq(
                &std::rc::Rc::downgrade(command.cmdlist()),
                &original_lifetime
            ));
            assert_eq!(command.key, 65);
            assert_eq!(command.flags, KEY_BINDING_REPEAT);
            assert_eq!(original_lifetime.strong_count(), 1);
            drop(table_owner);
            key_bindings_remove_table(name);
            assert!(key_bindings_get_table(name, 0).is_none());
            assert!(table_lifetime.upgrade().is_some());
            drop(retained_table);
            assert!(table_lifetime.upgrade().is_none());
            assert!(original_lifetime.upgrade().is_some());
            drop(command);
            assert!(original_lifetime.upgrade().is_none());
        }
    }
}
