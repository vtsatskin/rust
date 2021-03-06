// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! C definitions used by libnative that don't belong in liblibc

#![allow(type_overflow)]

use libc;

pub static WSADESCRIPTION_LEN: uint = 256;
pub static WSASYS_STATUS_LEN: uint = 128;
pub static FIONBIO: libc::c_long = 0x8004667e;
static FD_SETSIZE: uint = 64;
pub static MSG_DONTWAIT: libc::c_int = 0;

#[repr(C)]
pub struct WSADATA {
    pub wVersion: libc::WORD,
    pub wHighVersion: libc::WORD,
    pub szDescription: [u8, ..WSADESCRIPTION_LEN + 1],
    pub szSystemStatus: [u8, ..WSASYS_STATUS_LEN + 1],
    pub iMaxSockets: u16,
    pub iMaxUdpDg: u16,
    pub lpVendorInfo: *u8,
}

pub type LPWSADATA = *mut WSADATA;

#[repr(C)]
pub struct fd_set {
    fd_count: libc::c_uint,
    fd_array: [libc::SOCKET, ..FD_SETSIZE],
}

pub fn fd_set(set: &mut fd_set, s: libc::SOCKET) {
    set.fd_array[set.fd_count as uint] = s;
    set.fd_count += 1;
}

#[link(name = "ws2_32")]
extern "system" {
    pub fn WSAStartup(wVersionRequested: libc::WORD,
                      lpWSAData: LPWSADATA) -> libc::c_int;
    pub fn WSAGetLastError() -> libc::c_int;

    pub fn ioctlsocket(s: libc::SOCKET, cmd: libc::c_long,
                       argp: *mut libc::c_ulong) -> libc::c_int;
    pub fn select(nfds: libc::c_int,
                  readfds: *fd_set,
                  writefds: *fd_set,
                  exceptfds: *fd_set,
                  timeout: *libc::timeval) -> libc::c_int;
    pub fn getsockopt(sockfd: libc::SOCKET,
                      level: libc::c_int,
                      optname: libc::c_int,
                      optval: *mut libc::c_char,
                      optlen: *mut libc::c_int) -> libc::c_int;

    pub fn CancelIo(hFile: libc::HANDLE) -> libc::BOOL;
    pub fn CancelIoEx(hFile: libc::HANDLE,
                      lpOverlapped: libc::LPOVERLAPPED) -> libc::BOOL;
}

pub mod compat {
    use std::intrinsics::{atomic_store_relaxed, transmute};
    use libc::types::os::arch::extra::{LPCWSTR, HMODULE, LPCSTR, LPVOID};

    extern "system" {
        fn GetModuleHandleW(lpModuleName: LPCWSTR) -> HMODULE;
        fn GetProcAddress(hModule: HMODULE, lpProcName: LPCSTR) -> LPVOID;
    }

    // store_func() is idempotent, so using relaxed ordering for the atomics should be enough.
    // This way, calling a function in this compatibility layer (after it's loaded) shouldn't
    // be any slower than a regular DLL call.
    unsafe fn store_func<T: Copy>(ptr: *mut T, module: &str, symbol: &str, fallback: T) {
        let module = module.to_utf16().append_one(0);
        symbol.with_c_str(|symbol| {
            let handle = GetModuleHandleW(module.as_ptr());
            let func: Option<T> = transmute(GetProcAddress(handle, symbol));
            atomic_store_relaxed(ptr, func.unwrap_or(fallback))
        })
    }

    /// Macro for creating a compatibility fallback for a Windows function
    ///
    /// # Example
    /// ```
    /// compat_fn!(adll32::SomeFunctionW(_arg: LPCWSTR) {
    ///     // Fallback implementation
    /// })
    /// ```
    ///
    /// Note that arguments unused by the fallback implementation should not be called `_` as
    /// they are used to be passed to the real function if available.
    macro_rules! compat_fn(
        ($module:ident::$symbol:ident($($argname:ident: $argtype:ty),*)
                                      -> $rettype:ty $fallback:block) => (
            #[inline(always)]
            pub unsafe fn $symbol($($argname: $argtype),*) -> $rettype {
                static mut ptr: extern "system" fn($($argname: $argtype),*) -> $rettype = thunk;

                extern "system" fn thunk($($argname: $argtype),*) -> $rettype {
                    unsafe {
                        ::io::c::compat::store_func(&mut ptr,
                                                             stringify!($module),
                                                             stringify!($symbol),
                                                             fallback);
                        ::std::intrinsics::atomic_load_relaxed(&ptr)($($argname),*)
                    }
                }

                extern "system" fn fallback($($argname: $argtype),*) -> $rettype $fallback

                ::std::intrinsics::atomic_load_relaxed(&ptr)($($argname),*)
            }
        );

        ($module:ident::$symbol:ident($($argname:ident: $argtype:ty),*) $fallback:block) => (
            compat_fn!($module::$symbol($($argname: $argtype),*) -> () $fallback)
        )
    )

    /// Compatibility layer for functions in `kernel32.dll`
    ///
    /// Latest versions of Windows this is needed for:
    ///
    /// * `CreateSymbolicLinkW`: Windows XP, Windows Server 2003
    /// * `GetFinalPathNameByHandleW`: Windows XP, Windows Server 2003
    pub mod kernel32 {
        use libc::types::os::arch::extra::{DWORD, LPCWSTR, BOOLEAN, HANDLE};
        use libc::consts::os::extra::ERROR_CALL_NOT_IMPLEMENTED;

        extern "system" {
            fn SetLastError(dwErrCode: DWORD);
        }

        compat_fn!(kernel32::CreateSymbolicLinkW(_lpSymlinkFileName: LPCWSTR,
                                                 _lpTargetFileName: LPCWSTR,
                                                 _dwFlags: DWORD) -> BOOLEAN {
            unsafe { SetLastError(ERROR_CALL_NOT_IMPLEMENTED as DWORD); }
            0
        })

        compat_fn!(kernel32::GetFinalPathNameByHandleW(_hFile: HANDLE,
                                                       _lpszFilePath: LPCWSTR,
                                                       _cchFilePath: DWORD,
                                                       _dwFlags: DWORD) -> DWORD {
            unsafe { SetLastError(ERROR_CALL_NOT_IMPLEMENTED as DWORD); }
            0
        })
    }
}
