//! Authoritative signal declarations, supplied by the platform libc.

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ProcessSignal {
    Interrupt,
    Hangup,
    Child,
    Continue,
    Terminate,
    User1,
    User2,
    WindowChange,
    Other(::core::ffi::c_int),
}

impl ProcessSignal {
    pub fn from_raw(signal: ::core::ffi::c_int) -> Self {
        match signal {
            SIGINT => Self::Interrupt,
            SIGHUP => Self::Hangup,
            SIGCHLD => Self::Child,
            SIGCONT => Self::Continue,
            SIGTERM => Self::Terminate,
            SIGUSR1 => Self::User1,
            SIGUSR2 => Self::User2,
            SIGWINCH => Self::WindowChange,
            signal => Self::Other(signal),
        }
    }

    pub fn as_raw(self) -> ::core::ffi::c_int {
        match self {
            Self::Interrupt => SIGINT,
            Self::Hangup => SIGHUP,
            Self::Child => SIGCHLD,
            Self::Continue => SIGCONT,
            Self::Terminate => SIGTERM,
            Self::User1 => SIGUSR1,
            Self::User2 => SIGUSR2,
            Self::WindowChange => SIGWINCH,
            Self::Other(signal) => signal,
        }
    }
}

pub use ::libc::{
    sigset_t, SA_RESTART, SIGCHLD, SIGCONT, SIGHUP, SIGINT, SIGTERM, SIGTSTP, SIGTTIN, SIGTTOU,
    SIGUSR1, SIGUSR2, SIGWINCH, SIG_BLOCK, SIG_DFL, SIG_IGN, SIG_SETMASK,
};
