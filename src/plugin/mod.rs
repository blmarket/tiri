//! Server-owned providers for pane format variables. Plugin output never enters
//! user options. A refresh publishes a complete snapshot; format lookup only
//! reads that snapshot and does not call a plugin or inspect the filesystem.

use crate::src::reactor::Timer;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CStr, CString, OsStr};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub mod agent;
mod defaults;
pub mod git;
mod host;
#[cfg(test)]
mod tests;

use host::ServerHost;
pub use tiri_agent::observability::v1::{PaneId, ServerObservability};

/// An owned snapshot of the process and terminal state needed to classify a
/// non-agent pane. It contains no pane owner or borrowed terminal descriptor.
pub struct PaneActivity {
    pub foreground: Option<i32>,
    pub session_leader: Option<i32>,
    pub command: Option<CString>,
    pub alternate_on: bool,
    pub exited: bool,
}

/// Read-only server observations available to providers.
pub trait Host: ServerObservability {
    fn pane_cwd(&self, pane: PaneId) -> Option<PathBuf>;
    fn pane_activity(&self, pane: PaneId) -> Option<PaneActivity>;
}

/// A claimed name and its value before the first successful pane refresh.
#[derive(Clone, Copy)]
pub struct Variable {
    pub name: &'static CStr,
    pub default: &'static CStr,
}

impl Variable {
    pub const fn new(name: &'static CStr) -> Self {
        Self { name, default: c"" }
    }

    pub const fn with_default(name: &'static CStr, default: &'static CStr) -> Self {
        Self { name, default }
    }
}

type PaneMap = BTreeMap<u32, BTreeMap<&'static CStr, CString>>;

/// A single plugin's next snapshot. Every set is checked against that plugin's
/// declared names. Omitted panes and values are removed when the refresh commits.
pub struct PaneValues {
    variables: &'static [Variable],
    panes: PaneMap,
}

impl PaneValues {
    fn new(variables: &'static [Variable]) -> Self {
        Self {
            variables,
            panes: BTreeMap::new(),
        }
    }

    pub fn set(&mut self, pane: PaneId, key: &CStr, value: CString) -> io::Result<()> {
        let variable = self
            .variables
            .iter()
            .find(|variable| variable.name == key)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("undeclared plugin variable: {}", key.to_string_lossy()),
                )
            })?;
        self.panes
            .entry(pane.0)
            .or_default()
            .insert(variable.name, value);
        Ok(())
    }
}

/// Implement a provider, then register its sole owner with [`register`]. All
/// work runs on the server thread. An error preserves the previous snapshot.
pub trait Plugin {
    fn name(&self) -> &'static str;
    fn variables(&self) -> &'static [Variable];
    fn interval(&self) -> Duration;
    fn refresh(&mut self, host: &dyn Host, output: &mut PaneValues) -> io::Result<()>;
}

struct Slot {
    plugin: Option<Box<dyn Plugin>>,
    name: &'static str,
    variables: &'static [Variable],
    interval: Duration,
    due: Instant,
    values: PaneMap,
}

impl Slot {
    fn new(plugin: Box<dyn Plugin>) -> io::Result<Self> {
        let name = plugin.name();
        let variables = plugin.variables();
        let interval = plugin.interval();
        if name.is_empty() || interval.is_zero() || Instant::now().checked_add(interval).is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid plugin name or interval",
            ));
        }
        Ok(Self {
            plugin: Some(plugin),
            name,
            variables,
            interval,
            due: Instant::now(),
            values: BTreeMap::new(),
        })
    }
}

#[derive(Default)]
struct Registry {
    slots: Vec<Slot>,
    names: BTreeMap<&'static CStr, (usize, &'static CStr)>,
    timer: Option<Timer>,
}

impl Registry {
    fn insert(&mut self, prepared: &mut Option<Slot>) -> io::Result<()> {
        let slot = prepared.as_ref().expect("prepared plugin");
        let variables = slot.variables;
        if self
            .slots
            .iter()
            .any(|registered| registered.name == slot.name)
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "plugin already registered",
            ));
        }
        let mut declared = BTreeSet::new();
        for variable in variables {
            let bytes = variable.name.to_bytes();
            if bytes.is_empty()
                || !bytes
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
                || bytes[0].is_ascii_digit()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid plugin variable name",
                ));
            }
            if !declared.insert(variable.name)
                || self.names.contains_key(variable.name)
                || crate::src::format::format_is_builtin(variable.name)
                || crate::src::options_table::options_table
                    .iter()
                    .any(|entry| entry.name == Some(variable.name))
            {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!(
                        "format name already owned: {}",
                        variable.name.to_string_lossy()
                    ),
                ));
            }
        }
        let index = self.slots.len();
        for variable in variables {
            self.names.insert(variable.name, (index, variable.default));
        }
        self.slots.push(prepared.take().expect("prepared plugin"));
        Ok(())
    }

    fn find(&self, pane: Option<PaneId>, key: &CStr) -> Option<CString> {
        let &(index, default) = self.names.get(key)?;
        // A claimed variable remains claimed without pane context; do not fall
        // through to a same-named environment variable.
        let Some(pane) = pane else {
            return Some(CString::default());
        };
        Some(
            self.slots[index]
                .values
                .get(&pane.0)
                .and_then(|values| values.get(key))
                .map_or(default, CString::as_c_str)
                .to_owned(),
        )
    }

    fn publish(&mut self, index: usize, values: PaneValues) -> Vec<PaneId> {
        let slot = &mut self.slots[index];
        let changed: BTreeSet<_> = slot
            .values
            .keys()
            .chain(values.panes.keys())
            .filter(|id| slot.values.get(id) != values.panes.get(id))
            .copied()
            .collect();
        slot.values = values.panes;
        changed.into_iter().map(PaneId).collect()
    }
}

thread_local! {
    static REGISTRY: RefCell<Registry> = RefCell::new(Registry::default());
}

/// Register a compiled-in provider. Registration is atomic: any conflicting
/// name rejects the whole provider. No plugin code runs with a registry borrow.
///
/// # Safety
/// Call on the initialized server thread, after the daemon fork. Provider
/// observations require exclusive access to tmux's process-global model state.
pub unsafe fn register(plugin: Box<dyn Plugin>) -> io::Result<()> {
    let mut prepared = Some(Slot::new(plugin)?);
    REGISTRY.with(|registry| registry.borrow_mut().insert(&mut prepared))?;
    arm_next();
    Ok(())
}

pub(crate) fn find(pane: Option<PaneId>, key: &CStr) -> Option<CString> {
    REGISTRY.with(|registry| registry.borrow().find(pane, key))
}

pub(crate) fn each(pane: Option<PaneId>) -> Vec<(CString, CString)> {
    REGISTRY.with(|registry| {
        let registry = registry.borrow();
        registry
            .names
            .keys()
            .map(|key| {
                (
                    (*key).to_owned(),
                    registry.find(pane, key).expect("registered variable"),
                )
            })
            .collect()
    })
}

fn enabled_names(value: Option<&OsStr>) -> Vec<String> {
    let Some(value) = value else {
        return vec!["agent".into(), "git".into()];
    };
    let names: Vec<_> = value
        .as_bytes()
        .split(|b| *b == b',')
        .filter_map(|name| std::str::from_utf8(name).ok())
        .map(|name| name.trim().to_ascii_lowercase())
        .filter(|name| !name.is_empty())
        .collect();
    if names.iter().any(|name| name == "none") {
        Vec::new()
    } else {
        names
    }
}

/// Called in the server after fork and before configuration is loaded.
pub(crate) unsafe fn init() {
    let enabled = enabled_names(std::env::var_os("TMUX_C2RS_PLUGINS").as_deref());
    let builtins: [Box<dyn Plugin>; 2] = [
        Box::new(agent::AgentPlugin::new()),
        Box::new(git::GitPlugin::new()),
    ];
    for plugin in builtins {
        if enabled
            .iter()
            .any(|name| name == "all" || name == plugin.name())
        {
            register(plugin).expect("built-in plugin names are unique");
        }
    }
    if REGISTRY.with(|registry| !registry.borrow().slots.is_empty()) {
        defaults::apply();
    }
}

fn arm_next() {
    let next = REGISTRY.with(|registry| {
        let mut registry = registry.borrow_mut();
        let old = registry.timer.take();
        let due = registry
            .slots
            .iter()
            .filter(|slot| slot.plugin.is_some())
            .map(|slot| slot.due)
            .min();
        (old, due)
    });
    drop(next.0);
    if let Some(due) = next.1 {
        let timer =
            Timer::new(due.saturating_duration_since(Instant::now()), tick).expect("plugin timer");
        REGISTRY.with(|registry| registry.borrow_mut().timer = Some(timer));
    }
}

fn tick() {
    let timer = REGISTRY.with(|registry| registry.borrow_mut().timer.take());
    drop(timer);
    let now = Instant::now();
    let due: Vec<_> = REGISTRY.with(|registry| {
        registry
            .borrow()
            .slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.due <= now && slot.plugin.is_some())
            .map(|(index, _)| index)
            .collect()
    });
    let host = ServerHost;
    let mut redraw = BTreeSet::new();
    for index in due {
        let (mut plugin, variables) = REGISTRY.with(|registry| {
            let mut registry = registry.borrow_mut();
            let slot = &mut registry.slots[index];
            (
                slot.plugin.take().expect("scheduled plugin"),
                slot.variables,
            )
        });
        let mut values = PaneValues::new(variables);
        let result = plugin.refresh(&host, &mut values);
        if let Err(error) = &result {
            crate::src::log::log_debug(format_args!("plugin {}: {error}", plugin.name()));
        }
        let changed = REGISTRY.with(|registry| {
            let mut registry = registry.borrow_mut();
            let slot = &mut registry.slots[index];
            slot.plugin = Some(plugin);
            slot.due = Instant::now() + slot.interval;
            if result.is_ok() {
                registry.publish(index, values)
            } else {
                Vec::new()
            }
        });
        redraw.extend(changed.into_iter().map(|pane| pane.0));
    }
    // Published values are visible before any redraw can expand their formats.
    for id in redraw {
        host::invalidate(PaneId(id));
    }
    arm_next();
}

pub(crate) fn shutdown() {
    let mut old = REGISTRY.with(|registry| std::mem::take(&mut *registry.borrow_mut()));
    drop(old.timer.take());
    drop(old);
}
