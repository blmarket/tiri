use crate::src::log::{log_cstr, log_pointer};
use crate::src::server_client::ClientRegistry;
use crate::src::shared::client::{ClientRef, ClientWeak};
// Private job-integration implementation.  This module owns the process-wide
// format-job cache, per-client cache interaction, job callbacks, and tidy
// lifecycle.  It calls the parent facade for expansion, logging, and allocation.
// All cache borrows finish before format expansion or process callbacks.
use super::*;
use std::ffi::{CStr, CString};

fn format_job_set_expanded(fj: &mut format_job, value: CString) {
    fj.expanded = Default::default();
    fj.expanded = Some(value);
}

fn format_job_set_out(fj: &mut format_job, value: CString) {
    fj.out = Default::default();
    fj.out = Some(value);
}

// Match C-string visibility while keeping the line's Rust allocation local.
fn format_job_set_out_from_line(fj: &mut format_job, value: &[u8]) {
    let visible = value
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(value.len());
    let owned = CString::new(&value[..visible]).expect("visible job output contains no NUL");
    format_job_set_out(fj, owned);
}

fn format_job_message(fj: &format_job, suffix: &[u8]) -> CString {
    let cmd = fj.cmd.as_bytes();
    let mut bytes = Vec::with_capacity(2 + cmd.len() + suffix.len());
    bytes.extend_from_slice(b"<'");
    bytes.extend_from_slice(cmd);
    bytes.extend_from_slice(suffix);
    CString::new(bytes).expect("C command and literal suffix contain no NUL")
}

// Cache storage is owned by Client or by the process. Callbacks carry a weak
// Client and entry identity, never a reference or pointer into either cache.
thread_local! {
    static FORMAT_JOBS: std::cell::RefCell<format_job_tree> =
        const { std::cell::RefCell::new(format_job_tree::new()) };
}

#[derive(Clone)]
enum JobCache {
    Global,
    Client(ClientWeak),
}

impl JobCache {
    fn for_client(client: Option<&ClientRef>) -> Self {
        client.map_or(Self::Global, |client| {
            Self::Client(std::rc::Rc::downgrade(client))
        })
    }

    /// Only immediate cache edits. The closure must not expand formats, launch
    /// or free processes, notify clients, or return borrowed components.
    unsafe fn with_cache<R>(&self, edit: impl FnOnce(&mut format_job_tree) -> R) -> Option<R> {
        match self {
            Self::Global => Some(FORMAT_JOBS.with_borrow_mut(edit)),
            Self::Client(observer) => {
                let client = observer.upgrade()?;
                let mut cache = client.borrow_format_jobs_mut();
                Some(edit(cache))
            }
        }
    }

    unsafe fn entry(&self, client: Option<&ClientRef>, tag: u_int, command: &CStr) -> JobEntry {
        let key = (tag, command.to_bytes().to_vec());
        let identity = self
            .with_cache(|cache| {
                let record = format_job_find_or_insert(cache, client, tag, command);
                std::rc::Rc::downgrade(&record.identity)
            })
            .expect("format context retains the cache client");
        JobEntry {
            cache: self.clone(),
            key,
            identity,
        }
    }
}

#[derive(Clone)]
struct JobEntry {
    cache: JobCache,
    key: (u_int, Vec<u8>),
    identity: std::rc::Weak<()>,
}

impl JobEntry {
    unsafe fn with_record<R>(&self, edit: impl FnOnce(&mut format_job) -> R) -> Option<R> {
        self.cache
            .with_cache(|cache| {
                let record = cache.get_mut(&self.key)?;
                if !std::rc::Rc::downgrade(&record.identity).ptr_eq(&self.identity) {
                    return None;
                }
                Some(edit(record))
            })
            .flatten()
    }
}

unsafe fn format_job_update(job: &refbox::Weak<job>, entry: &JobEntry) {
    let line = {
        let buffer = crate::src::reactor::bufferevent_get_input(&mut *job_get_event(job));
        let mut line = None;
        while let Some(next) = evbuffer_readline(buffer) {
            line = Some(next);
        }
        line
    };
    let Some(line) = line else { return };
    let Some((client, status, last)) = entry.with_record(|record| {
        record.updated = 1;
        format_job_set_out_from_line(record, &line);
        log_debug(format_args!(
            "format_job_update: {} {}: {}",
            log_pointer(std::ptr::from_ref(record)),
            log_cstr(&record.cmd),
            log_cstr(record.out.as_ref().unwrap()),
        ));
        (record.client.clone(), record.status, record.last)
    }) else {
        return;
    };
    let now = time(std::ptr::null_mut());
    if status != 0 && last != now {
        if let Some(client) = client.upgrade() {
            server_status_client(&client);
        }
        // Keep publication after the notification, as before. A notification
        // may have retired this entry, so reacquire and check its identity.
        entry.with_record(|record| record.last = now);
    }
}

unsafe fn format_job_complete(completion: JobCompletion, entry: &JobEntry) {
    let mut buffer = evbuffer_new();
    if !completion.output.is_empty() {
        evbuffer_add(&mut buffer, &completion.output);
    }
    if entry
        .with_record(|record| record.job = refbox::Weak::new())
        .is_none()
    {
        return;
    }
    let output = if let Some(line) = evbuffer_readline(&mut buffer) {
        let visible = line
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(line.len());
        CString::new(&line[..visible]).expect("visible job output contains no NUL")
    } else {
        let bytes = evbuffer_pullup(&mut buffer, -1).unwrap_or_default();
        let visible = bytes
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(bytes.len());
        CString::new(&bytes[..visible]).expect("visible job output contains no NUL")
    };
    let Some((client, status)) = entry.with_record(|record| {
        log_debug(format_args!(
            "format_job_complete: {} {}: {}",
            log_pointer(std::ptr::from_ref(record)),
            log_cstr(&record.cmd),
            log_cstr(&output),
        ));
        if !output.is_empty() || record.updated == 0 {
            format_job_set_out(record, output);
        }
        (record.client.clone(), record.status)
    }) else {
        return;
    };
    if status != 0 {
        if let Some(client) = client.upgrade() {
            server_status_client(&client);
        }
        entry.with_record(|record| record.status = 0);
    }
}

pub(super) unsafe fn format_job_get(es: *mut format_expand_state, command: &CStr) -> CString {
    let ft = (*es).ft;
    let cache = JobCache::for_client((*ft).client.as_ref());
    // Publish the entry before expansion, preserving recursive lookup ordering.
    let entry = cache.entry((*ft).client.as_ref(), (*ft).tag, command);
    let mut next = format_expand_state::default();
    format_copy_state(&mut next, es, FORMAT_EXPAND_NOJOBS | FORMAT_EXPAND_NOCYCLE);
    next.flags &= !FORMAT_EXPAND_TIME;
    let expanded = format_expand1_cstring(&mut next, command.as_ptr());
    let Some((force, running)) = entry.with_record(|record| {
        let force = if record.expanded.as_deref() != Some(expanded.as_c_str()) {
            format_job_set_expanded(record, expanded.clone());
            1
        } else {
            (*ft).flags & FORMAT_FORCE
        };
        (force, record.job.clone())
    }) else {
        return CString::default();
    };
    let now = time(std::ptr::null_mut());
    if force != 0 && !running.is_empty() {
        job_free(&running);
    }
    let Some(start) =
        entry.with_record(|record| force != 0 || record.job.is_empty() && record.last != now)
    else {
        return CString::default();
    };
    if start {
        let cwd = ClientRef::working_directory((*ft).client.as_ref(), None);
        let update_entry = entry.clone();
        let complete_entry = entry.clone();
        let started = job_run(
            Some(&expanded),
            &Vec::new(),
            None,
            None,
            cwd.as_deref(),
            job_update_callback(move |job| unsafe { format_job_update(job, &update_entry) }),
            Some(Box::new(move |completion| unsafe {
                format_job_complete(completion, &complete_entry)
            })),
            None,
            JOB_NOWAIT,
            -1,
            -1,
        );
        if entry
            .with_record(|record| {
                record.job = started.clone();
                if record.job.is_empty() {
                    let message = format_job_message(record, b"' didn't start>");
                    format_job_set_out(record, message);
                }
                record.last = now;
                record.updated = 0;
            })
            .is_none()
        {
            // A removed cache cannot own a newly started process. Keep explicit
            // cancellation outside the borrow even on this reentrant path.
            job_free(&started);
            return CString::default();
        }
    } else {
        entry.with_record(|record| {
            if !record.job.is_empty() && now - record.last > 1 && record.out.is_none() {
                let message = format_job_message(record, b"' not ready>");
                format_job_set_out(record, message);
            }
        });
    }
    let output = entry
        .with_record(|record| {
            if (*ft).flags & FORMAT_STATUS != 0 {
                record.status = 1;
            }
            record.out.clone()
        })
        .flatten();
    output.map_or_else(CString::default, |output| {
        format_expand1_cstring(&mut next, output.as_ptr())
    })
}

// Component-only lookup: callers keep this reference inside their cache borrow.
fn format_job_find_or_insert<'a>(
    jobs: &'a mut format_job_tree,
    client: Option<&ClientRef>,
    tag: u_int,
    command: &CStr,
) -> &'a mut format_job {
    jobs.entry((tag, command.to_bytes().to_vec()))
        .or_insert_with(|| {
            Box::new(format_job {
                identity: std::rc::Rc::new(()),
                client: client.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
                tag,
                cmd: command.to_owned(),
                expanded: None,
                last: 0,
                out: None,
                updated: 0,
                job: refbox::Weak::new(),
                status: 0,
            })
        })
}

unsafe fn format_job_tidy_at(cache: &JobCache, force: i32, now: time_t) {
    let expired = cache
        .with_cache(|cache| {
            cache
                .iter()
                .filter_map(|(key, record)| {
                    if force == 0 && (record.last > now || now - record.last < 3600) {
                        None
                    } else {
                        Some((key.clone(), std::rc::Rc::downgrade(&record.identity)))
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for (key, identity) in expired {
        let removed = cache
            .with_cache(|cache| {
                let jobs = cache;
                let record = jobs.get(&key)?;
                if !std::rc::Rc::downgrade(&record.identity).ptr_eq(&identity) {
                    return None;
                }
                jobs.remove(&key)
            })
            .flatten();
        let Some(record) = removed else { continue };
        log_debug(format_args!("format_job_tidy: {}", log_cstr(&record.cmd)));
        if !record.job.is_empty() {
            job_free(&record.job);
        }
        drop(record);
    }
}

pub unsafe fn format_tidy_jobs() {
    format_job_tidy_at(&JobCache::Global, 0, time(std::ptr::null_mut()));
    let mut current = ClientRegistry::first();
    while let Some(client) = current {
        format_job_tidy_at(
            &JobCache::for_client(Some(&client)),
            0,
            time(std::ptr::null_mut()),
        );
        current = ClientRegistry::next(&client);
    }
}

pub unsafe fn format_lost_client(client: &ClientRef) {
    let cache = JobCache::for_client(Some(client));
    // Keep the cache installed throughout explicit process cancellation.
    format_job_tidy_at(&cache, 1, time(std::ptr::null_mut()));
    // Cancellation callbacks may have inserted replacements. Detach each batch
    // before explicit cleanup, so process callbacks never overlap a cache loan.
    loop {
        let removed = std::mem::take(&mut *client.borrow_format_jobs_mut());
        if removed.is_empty() {
            break;
        }
        for record in removed.into_values() {
            if !record.job.is_empty() {
                job_free(&record.job);
            }
            drop(record);
        }
    }
}
