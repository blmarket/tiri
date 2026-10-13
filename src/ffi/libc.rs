//! Foreign declarations supplied by libc.
//! Declarations whose types or symbols vary by platform come from the libc crate.

use crate::src::shared::abi::{__off_t, __pid_t, __uid_t, size_t, ssize_t, time_t};
use crate::src::shared::stdio::FILE;
use crate::src::shared::utf8::wchar_t;
#[cfg(target_os = "linux")]
pub use ::libc::__errno_location;
#[cfg(target_vendor = "apple")]
pub use ::libc::__error as __errno_location;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub use ::libc::malloc_trim;
pub use ::libc::{
    cfgetispeed, cfgetospeed, cfmakeraw, cfsetispeed, cfsetospeed, chmod, clock_gettime, ctime_r,
    forkpty, getgrgid, getgrnam, getpwnam, getpwuid, glob, globfree, gmtime_r, localtime,
    localtime_r, lstat, mkdir, sigaction, sigemptyset, sigfillset, sigprocmask, stat, strftime,
    umask, uname, utsname,
};

#[cfg(target_os = "linux")]
extern "C" {
    pub static mut program_invocation_short_name: *mut ::core::ffi::c_char;
}

pub type __useconds_t = ::core::ffi::c_uint;

pub type nl_item = ::core::ffi::c_int;

extern "C" {
    /// POSIX basename, which glibc exports under its XPG name.
    #[cfg_attr(
        all(target_os = "linux", target_env = "gnu"),
        link_name = "__xpg_basename"
    )]
    pub fn basename(__path: *mut ::core::ffi::c_char) -> *mut ::core::ffi::c_char;
    #[cfg_attr(target_vendor = "apple", link_name = "__stderrp")]
    pub static mut stderr: *mut FILE;
    #[cfg_attr(target_vendor = "apple", link_name = "__stdoutp")]
    pub static mut stdout: *mut FILE;
    pub fn _exit(__status: ::core::ffi::c_int) -> !;
    pub fn abs(__x: ::core::ffi::c_int) -> ::core::ffi::c_int;
    pub fn access(
        __name: *const ::core::ffi::c_char,
        __type: ::core::ffi::c_int,
    ) -> ::core::ffi::c_int;
    pub fn calloc(__nmemb: size_t, __size: size_t) -> *mut ::core::ffi::c_void;
    pub fn chdir(__path: *const ::core::ffi::c_char) -> ::core::ffi::c_int;
    pub fn daemon(
        __nochdir: ::core::ffi::c_int,
        __noclose: ::core::ffi::c_int,
    ) -> ::core::ffi::c_int;
    pub fn dirname(__path: *mut ::core::ffi::c_char) -> *mut ::core::ffi::c_char;
    pub static mut environ: *mut *mut ::core::ffi::c_char;
    pub fn err(_: ::core::ffi::c_int, _: *const ::core::ffi::c_char, ...);
    pub fn errx(_: ::core::ffi::c_int, _: *const ::core::ffi::c_char, ...);
    pub fn execl(
        __path: *const ::core::ffi::c_char,
        __arg: *const ::core::ffi::c_char,
        ...
    ) -> ::core::ffi::c_int;
    pub fn execvp(
        __file: *const ::core::ffi::c_char,
        __argv: *const *mut ::core::ffi::c_char,
    ) -> ::core::ffi::c_int;
    pub fn exit(__status: ::core::ffi::c_int) -> !;
    pub fn fclose(__stream: *mut FILE) -> ::core::ffi::c_int;
    pub fn fdopen(__fd: ::core::ffi::c_int, __modes: *const ::core::ffi::c_char) -> *mut FILE;
    pub fn ferror(__stream: *mut FILE) -> ::core::ffi::c_int;
    pub fn fflush(__stream: *mut FILE) -> ::core::ffi::c_int;
    pub fn fgetc(__stream: *mut FILE) -> ::core::ffi::c_int;
    pub fn fnmatch(
        __pattern: *const ::core::ffi::c_char,
        __name: *const ::core::ffi::c_char,
        __flags: ::core::ffi::c_int,
    ) -> ::core::ffi::c_int;
    pub fn fopen(
        __filename: *const ::core::ffi::c_char,
        __modes: *const ::core::ffi::c_char,
    ) -> *mut FILE;
    pub fn fork() -> __pid_t;
    pub fn fprintf(
        __stream: *mut FILE,
        __format: *const ::core::ffi::c_char,
        ...
    ) -> ::core::ffi::c_int;
    pub fn fputc(__c: ::core::ffi::c_int, __stream: *mut FILE) -> ::core::ffi::c_int;
    pub fn fputs(__s: *const ::core::ffi::c_char, __stream: *mut FILE) -> ::core::ffi::c_int;
    pub fn fread(
        __ptr: *mut ::core::ffi::c_void,
        __size: size_t,
        __n: size_t,
        __stream: *mut FILE,
    ) -> ::core::ffi::c_ulong;
    pub fn free(__ptr: *mut ::core::ffi::c_void);
    pub fn fseeko(
        __stream: *mut FILE,
        __off: __off_t,
        __whence: ::core::ffi::c_int,
    ) -> ::core::ffi::c_int;
    pub fn ftello(__stream: *mut FILE) -> __off_t;
    pub fn fwrite(
        __ptr: *const ::core::ffi::c_void,
        __size: size_t,
        __n: size_t,
        __s: *mut FILE,
    ) -> ::core::ffi::c_ulong;
    pub fn getc(__stream: *mut FILE) -> ::core::ffi::c_int;
    pub fn getcwd(__buf: *mut ::core::ffi::c_char, __size: size_t) -> *mut ::core::ffi::c_char;
    pub fn getenv(__name: *const ::core::ffi::c_char) -> *mut ::core::ffi::c_char;
    pub fn gethostname(__name: *mut ::core::ffi::c_char, __len: size_t) -> ::core::ffi::c_int;
    pub fn getpid() -> __pid_t;
    pub fn getppid() -> __pid_t;
    pub fn getuid() -> __uid_t;
    pub fn kill(__pid: __pid_t, __sig: ::core::ffi::c_int) -> ::core::ffi::c_int;
    pub fn killpg(__pgrp: __pid_t, __sig: ::core::ffi::c_int) -> ::core::ffi::c_int;
    pub fn llabs(__x: ::core::ffi::c_longlong) -> ::core::ffi::c_longlong;
    pub fn memchr(
        __s: *const ::core::ffi::c_void,
        __c: ::core::ffi::c_int,
        __n: size_t,
    ) -> *mut ::core::ffi::c_void;
    pub fn memcmp(
        __s1: *const ::core::ffi::c_void,
        __s2: *const ::core::ffi::c_void,
        __n: size_t,
    ) -> ::core::ffi::c_int;
    pub fn memcpy(
        __dest: *mut ::core::ffi::c_void,
        __src: *const ::core::ffi::c_void,
        __n: size_t,
    ) -> *mut ::core::ffi::c_void;
    pub fn memmove(
        __dest: *mut ::core::ffi::c_void,
        __src: *const ::core::ffi::c_void,
        __n: size_t,
    ) -> *mut ::core::ffi::c_void;
    pub fn memset(
        __s: *mut ::core::ffi::c_void,
        __c: ::core::ffi::c_int,
        __n: size_t,
    ) -> *mut ::core::ffi::c_void;
    pub fn mkstemp(__template: *mut ::core::ffi::c_char) -> ::core::ffi::c_int;
    pub fn nl_langinfo(__item: nl_item) -> *mut ::core::ffi::c_char;
    pub fn printf(__format: *const ::core::ffi::c_char, ...) -> ::core::ffi::c_int;
    pub fn readlink(
        __path: *const ::core::ffi::c_char,
        __buf: *mut ::core::ffi::c_char,
        __len: size_t,
    ) -> ssize_t;
    pub fn realpath(
        __name: *const ::core::ffi::c_char,
        __resolved: *mut ::core::ffi::c_char,
    ) -> *mut ::core::ffi::c_char;
    pub fn setenv(
        __name: *const ::core::ffi::c_char,
        __value: *const ::core::ffi::c_char,
        __replace: ::core::ffi::c_int,
    ) -> ::core::ffi::c_int;
    pub fn setlocale(
        __category: ::core::ffi::c_int,
        __locale: *const ::core::ffi::c_char,
    ) -> *mut ::core::ffi::c_char;
    pub fn setpgid(__pid: __pid_t, __pgid: __pid_t) -> ::core::ffi::c_int;
    pub fn setvbuf(
        __stream: *mut FILE,
        __buf: *mut ::core::ffi::c_char,
        __modes: ::core::ffi::c_int,
        __n: size_t,
    ) -> ::core::ffi::c_int;
    pub fn snprintf(
        __s: *mut ::core::ffi::c_char,
        __maxlen: size_t,
        __format: *const ::core::ffi::c_char,
        ...
    ) -> ::core::ffi::c_int;
    pub fn sscanf(
        __s: *const ::core::ffi::c_char,
        __format: *const ::core::ffi::c_char,
        ...
    ) -> ::core::ffi::c_int;
    pub fn strcasecmp(
        __s1: *const ::core::ffi::c_char,
        __s2: *const ::core::ffi::c_char,
    ) -> ::core::ffi::c_int;
    pub fn strcasestr(
        __haystack: *const ::core::ffi::c_char,
        __needle: *const ::core::ffi::c_char,
    ) -> *mut ::core::ffi::c_char;
    pub fn strchr(
        __s: *const ::core::ffi::c_char,
        __c: ::core::ffi::c_int,
    ) -> *mut ::core::ffi::c_char;
    pub fn strcmp(
        __s1: *const ::core::ffi::c_char,
        __s2: *const ::core::ffi::c_char,
    ) -> ::core::ffi::c_int;
    pub fn strcspn(
        __s: *const ::core::ffi::c_char,
        __reject: *const ::core::ffi::c_char,
    ) -> ::core::ffi::c_ulong;
    pub fn strerror(__errnum: ::core::ffi::c_int) -> *mut ::core::ffi::c_char;
    pub fn strlcat(
        __dest: *mut ::core::ffi::c_char,
        __src: *const ::core::ffi::c_char,
        __n: size_t,
    ) -> ::core::ffi::c_ulong;
    pub fn strlcpy(
        __dest: *mut ::core::ffi::c_char,
        __src: *const ::core::ffi::c_char,
        __n: size_t,
    ) -> ::core::ffi::c_ulong;
    pub fn strlen(__s: *const ::core::ffi::c_char) -> size_t;
    pub fn strncasecmp(
        __s1: *const ::core::ffi::c_char,
        __s2: *const ::core::ffi::c_char,
        __n: size_t,
    ) -> ::core::ffi::c_int;
    pub fn strncmp(
        __s1: *const ::core::ffi::c_char,
        __s2: *const ::core::ffi::c_char,
        __n: size_t,
    ) -> ::core::ffi::c_int;
    pub fn strpbrk(
        __s: *const ::core::ffi::c_char,
        __accept: *const ::core::ffi::c_char,
    ) -> *mut ::core::ffi::c_char;
    pub fn strrchr(
        __s: *const ::core::ffi::c_char,
        __c: ::core::ffi::c_int,
    ) -> *mut ::core::ffi::c_char;
    pub fn strsep(
        __stringp: *mut *mut ::core::ffi::c_char,
        __delim: *const ::core::ffi::c_char,
    ) -> *mut ::core::ffi::c_char;
    pub fn strsignal(__sig: ::core::ffi::c_int) -> *mut ::core::ffi::c_char;
    pub fn strspn(
        __s: *const ::core::ffi::c_char,
        __accept: *const ::core::ffi::c_char,
    ) -> ::core::ffi::c_ulong;
    pub fn strstr(
        __haystack: *const ::core::ffi::c_char,
        __needle: *const ::core::ffi::c_char,
    ) -> *mut ::core::ffi::c_char;
    pub fn strtod(
        __nptr: *const ::core::ffi::c_char,
        __endptr: *mut *mut ::core::ffi::c_char,
    ) -> ::core::ffi::c_double;
    pub fn strtol(
        __nptr: *const ::core::ffi::c_char,
        __endptr: *mut *mut ::core::ffi::c_char,
        __base: ::core::ffi::c_int,
    ) -> ::core::ffi::c_long;
    pub fn strtoll(
        __nptr: *const ::core::ffi::c_char,
        __endptr: *mut *mut ::core::ffi::c_char,
        __base: ::core::ffi::c_int,
    ) -> ::core::ffi::c_longlong;
    pub fn strtoul(
        __nptr: *const ::core::ffi::c_char,
        __endptr: *mut *mut ::core::ffi::c_char,
        __base: ::core::ffi::c_int,
    ) -> ::core::ffi::c_ulong;
    pub fn strtoull(
        __nptr: *const ::core::ffi::c_char,
        __endptr: *mut *mut ::core::ffi::c_char,
        __base: ::core::ffi::c_int,
    ) -> ::core::ffi::c_ulonglong;
    pub fn system(__command: *const ::core::ffi::c_char) -> ::core::ffi::c_int;
    pub fn time(__timer: *mut time_t) -> time_t;
    pub fn tzset();
    pub fn ungetc(__c: ::core::ffi::c_int, __stream: *mut FILE) -> ::core::ffi::c_int;
    pub fn unlink(__name: *const ::core::ffi::c_char) -> ::core::ffi::c_int;
    pub fn unsetenv(__name: *const ::core::ffi::c_char) -> ::core::ffi::c_int;
    pub fn usleep(__useconds: __useconds_t) -> ::core::ffi::c_int;
    pub fn vasprintf(
        __ptr: *mut *mut ::core::ffi::c_char,
        __f: *const ::core::ffi::c_char,
        __arg: ::core::ffi::VaList,
    ) -> ::core::ffi::c_int;
    pub fn vsnprintf(
        __s: *mut ::core::ffi::c_char,
        __maxlen: size_t,
        __format: *const ::core::ffi::c_char,
        __arg: ::core::ffi::VaList,
    ) -> ::core::ffi::c_int;
    pub fn waitpid(
        __pid: __pid_t,
        __stat_loc: *mut ::core::ffi::c_int,
        __options: ::core::ffi::c_int,
    ) -> __pid_t;
    pub fn warnx(_: *const ::core::ffi::c_char, ...);
    pub fn wctomb(__s: *mut ::core::ffi::c_char, __wchar: wchar_t) -> ::core::ffi::c_int;
}
