//! Owned, blocking sd-bus client handles.
#![deny(unsafe_op_in_unsafe_fn)]

use libsystemd_sys::bus::{
    sd_bus, sd_bus_add_match, sd_bus_call, sd_bus_default_user, sd_bus_error, sd_bus_error_free,
    sd_bus_message, sd_bus_message_append_basic, sd_bus_message_close_container,
    sd_bus_message_new_method_call, sd_bus_message_open_container, sd_bus_message_read_basic,
    sd_bus_message_ref, sd_bus_message_unref, sd_bus_process, sd_bus_slot, sd_bus_slot_unref,
    sd_bus_unref, sd_bus_wait,
};
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::io;
use std::ptr::{self, NonNull};
use std::time::Duration;

fn check(r: c_int) -> io::Result<c_int> {
    if r < 0 {
        Err(io::Error::from_raw_os_error(-r))
    } else {
        Ok(r)
    }
}

fn usec(timeout: Duration) -> u64 {
    timeout.as_micros().min(u64::MAX as u128) as u64
}

/// A connection to a bus.
pub struct Bus(NonNull<sd_bus>);

impl Bus {
    /// Connect to the calling user's session bus.
    pub fn default_user() -> io::Result<Bus> {
        let mut bus = ptr::null_mut();
        check(unsafe { sd_bus_default_user(&raw mut bus) })?;
        Ok(Bus(
            NonNull::new(bus).expect("sd_bus_default_user returns a bus")
        ))
    }

    /// Create a method call message addressed to `destination`.
    pub fn new_method_call(
        &self,
        destination: &CStr,
        path: &CStr,
        interface: &CStr,
        member: &CStr,
    ) -> io::Result<Message> {
        let mut m = ptr::null_mut();
        check(unsafe {
            sd_bus_message_new_method_call(
                self.0.as_ptr(),
                &raw mut m,
                destination.as_ptr(),
                path.as_ptr(),
                interface.as_ptr(),
                member.as_ptr(),
            )
        })?;
        Ok(Message(NonNull::new(m).expect(
            "sd_bus_message_new_method_call returns a message",
        )))
    }

    /// Send `message` and block for its reply.
    pub fn call(&self, message: &Message, timeout: Duration) -> Result<Message, CallError> {
        let mut error = sd_bus_error {
            name: ptr::null(),
            message: ptr::null(),
            need_free: 0,
        };
        let mut reply = ptr::null_mut();
        let r = unsafe {
            sd_bus_call(
                self.0.as_ptr(),
                message.0.as_ptr(),
                usec(timeout),
                &raw mut error,
                &raw mut reply,
            )
        };
        let result = match NonNull::new(reply) {
            Some(reply) if r >= 0 => Ok(Message(reply)),
            _ => Err(CallError {
                error: io::Error::from_raw_os_error(if r < 0 { -r } else { libc::EBADMSG }),
                message: (!error.message.is_null())
                    .then(|| unsafe { CStr::from_ptr(error.message) }.to_owned()),
            }),
        };
        unsafe { sd_bus_error_free(&raw mut error) };
        result
    }

    /// Run `body` with `handler` called for each message matching `rule`.
    /// Handlers run only from [`Bus::process`] calls made inside `body`. The
    /// match is removed before this returns, even if `body` panics.
    pub fn with_match<F, R>(
        &self,
        rule: &CStr,
        mut handler: F,
        body: impl FnOnce() -> R,
    ) -> io::Result<R>
    where
        F: FnMut(&mut Message) -> io::Result<()>,
    {
        unsafe extern "C" fn trampoline<F: FnMut(&mut Message) -> io::Result<()>>(
            m: *mut sd_bus_message,
            userdata: *mut c_void,
            _ret_error: *mut sd_bus_error,
        ) -> c_int {
            // SAFETY: userdata is `handler` below, which outlives the slot.
            // sd-bus refuses nested `sd_bus_process`, so this is the only
            // live reference.
            let handler = unsafe { &mut *(userdata as *mut F) };
            let mut message = Message(
                NonNull::new(unsafe { sd_bus_message_ref(m) }).expect("handler receives a message"),
            );
            match handler(&mut message) {
                Ok(()) => 0,
                Err(error) => -error.raw_os_error().unwrap_or(libc::EIO),
            }
        }

        /// Unregisters the match on scope exit, including unwinding.
        struct Slot(NonNull<sd_bus_slot>);
        impl Drop for Slot {
            fn drop(&mut self) {
                // The only reference: never exposed, so never dispatching.
                unsafe { sd_bus_slot_unref(self.0.as_ptr()) };
            }
        }

        let mut slot = ptr::null_mut();
        check(unsafe {
            sd_bus_add_match(
                self.0.as_ptr(),
                &raw mut slot,
                rule.as_ptr(),
                Some(trampoline::<F>),
                &raw mut handler as *mut c_void,
            )
        })?;
        let _slot = Slot(NonNull::new(slot).expect("sd_bus_add_match returns a slot"));
        Ok(body())
    }

    /// Process one pending message. Returns false when nothing was pending.
    pub fn process(&self) -> io::Result<bool> {
        Ok(check(unsafe { sd_bus_process(self.0.as_ptr(), ptr::null_mut()) })? > 0)
    }

    /// Block until the bus has work to process, or `timeout` passes. Returns
    /// false on timeout.
    pub fn wait(&self, timeout: Duration) -> io::Result<bool> {
        Ok(check(unsafe { sd_bus_wait(self.0.as_ptr(), usec(timeout)) })? > 0)
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        unsafe { sd_bus_unref(self.0.as_ptr()) };
    }
}

/// A failed method call.
#[derive(Debug)]
pub struct CallError {
    pub error: io::Error,
    /// The human readable message of the D-Bus error reply, if any.
    pub message: Option<CString>,
}

/// A D-Bus value carried in a variant.
pub enum Variant<'a> {
    Str(&'a CStr),
    Bool(bool),
    U32Array(&'a [u32]),
    StrArray(&'a [&'a CStr]),
}

/// A message being built or read.
pub struct Message(NonNull<sd_bus_message>);

impl Message {
    fn append_basic(&mut self, kind: u8, value: *const c_void) -> io::Result<()> {
        check(unsafe { sd_bus_message_append_basic(self.0.as_ptr(), kind as c_char, value) })?;
        Ok(())
    }

    pub fn append_str(&mut self, value: &CStr) -> io::Result<()> {
        self.append_basic(b's', value.as_ptr() as *const c_void)
    }

    pub fn append_bool(&mut self, value: bool) -> io::Result<()> {
        let value = value as c_int;
        self.append_basic(b'b', &raw const value as *const c_void)
    }

    pub fn append_u32(&mut self, value: u32) -> io::Result<()> {
        self.append_basic(b'u', &raw const value as *const c_void)
    }

    /// Open an array (`a`), struct (`r`), variant (`v`) or dict entry (`e`)
    /// whose contents have the signature `contents`.
    pub fn open_container(&mut self, kind: u8, contents: &CStr) -> io::Result<()> {
        check(unsafe {
            sd_bus_message_open_container(self.0.as_ptr(), kind as c_char, contents.as_ptr())
        })?;
        Ok(())
    }

    pub fn close_container(&mut self) -> io::Result<()> {
        check(unsafe { sd_bus_message_close_container(self.0.as_ptr()) })?;
        Ok(())
    }

    /// Append one `(sv)` entry of a property array.
    pub fn append_property(&mut self, name: &CStr, value: Variant<'_>) -> io::Result<()> {
        self.open_container(b'r', c"sv")?;
        self.append_str(name)?;
        match value {
            Variant::Str(value) => {
                self.open_container(b'v', c"s")?;
                self.append_str(value)?;
            }
            Variant::Bool(value) => {
                self.open_container(b'v', c"b")?;
                self.append_bool(value)?;
            }
            Variant::U32Array(values) => {
                self.open_container(b'v', c"au")?;
                self.open_container(b'a', c"u")?;
                for &value in values {
                    self.append_u32(value)?;
                }
                self.close_container()?;
            }
            Variant::StrArray(values) => {
                self.open_container(b'v', c"as")?;
                self.open_container(b'a', c"s")?;
                for value in values {
                    self.append_str(value)?;
                }
                self.close_container()?;
            }
        }
        self.close_container()?;
        self.close_container()
    }

    fn read_basic(&mut self, kind: u8, value: *mut c_void) -> io::Result<()> {
        match check(unsafe { sd_bus_message_read_basic(self.0.as_ptr(), kind as c_char, value) })? {
            0 => Err(io::Error::from_raw_os_error(libc::EBADMSG)),
            _ => Ok(()),
        }
    }

    pub fn read_u32(&mut self) -> io::Result<u32> {
        let mut value = 0u32;
        self.read_basic(b'u', &raw mut value as *mut c_void)?;
        Ok(value)
    }

    /// Read an object path. It is borrowed from the message.
    pub fn read_object_path(&mut self) -> io::Result<&CStr> {
        let mut value: *const c_char = ptr::null();
        self.read_basic(b'o', &raw mut value as *mut c_void)?;
        Ok(unsafe { CStr::from_ptr(value) })
    }
}

impl Drop for Message {
    fn drop(&mut self) {
        unsafe { sd_bus_message_unref(self.0.as_ptr()) };
    }
}
