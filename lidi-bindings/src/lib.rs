#![allow(unsafe_code)]

use lidi_clients as clients;
use std::{
    ffi::{CStr, c_char},
    net::SocketAddr,
    path::PathBuf,
    ptr,
    str::FromStr,
};

/// # Panics
///
/// Will return `Err` if ip and port cannot be parsed.
#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn diode_new_config(
    ptr_addr: *const c_char,
    buffer_size: u32,
) -> *mut clients::file::Config<clients::DiodeSend> {
    if ptr_addr.is_null() {
        return ptr::null_mut();
    }
    let cstr_addr = unsafe { CStr::from_ptr(ptr_addr) };
    let rust_addr = String::from_utf8_lossy(cstr_addr.to_bytes()).to_string();
    let socket_addr = SocketAddr::from_str(&rust_addr).expect("ip:port");

    let config = Box::new(clients::file::Config {
        diode: clients::DiodeSend::Tcp(socket_addr),
        buffer_size: buffer_size as usize,
        hash: false,
        max_files: 0,
    });
    Box::into_raw(config)
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn diode_free_config(ptr: *mut clients::file::Config<clients::DiodeSend>) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(ptr));
    }
}

/// # Panics
///
/// Will return `Err` if reference to `config` is wrong.
#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn diode_send_file(
    ptr: *mut clients::file::Config<clients::DiodeSend>,
    ptr_filepath: *const c_char,
) -> u32 {
    if ptr.is_null() {
        return 0;
    }
    let config = unsafe { ptr.as_ref() }.expect("config");

    if ptr_filepath.is_null() {
        return 0;
    }
    let cstr_filepath = unsafe { CStr::from_ptr(ptr_filepath) };
    let rust_filepath = String::from_utf8_lossy(cstr_filepath.to_bytes()).to_string();

    let result: usize = clients::file::send::send_file(config, &rust_filepath).unwrap_or(0);
    u32::try_from(result).unwrap_or(0)
}

/// # Panics
///
/// Will return `Err` if reference to `config` is wrong.
#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn diode_receive_files(
    ptr: *mut clients::file::Config<clients::DiodeSend>,
    ptr_odir: *const c_char,
) {
    if ptr.is_null() {
        return;
    }
    let config = unsafe { ptr.as_ref() }.expect("config");
    let clients::DiodeSend::Tcp(socket_addr) = config.diode else {
        return;
    };

    let config = clients::file::Config {
        diode: clients::DiodeReceive {
            from_tcp: Some(socket_addr),
            from_unix: None,
        },
        buffer_size: config.buffer_size,
        hash: false,
        max_files: 0,
    };

    if ptr_odir.is_null() {
        return;
    }
    let cstr_odir = unsafe { CStr::from_ptr(ptr_odir) };
    let rust_odir = String::from_utf8_lossy(cstr_odir.to_bytes()).to_string();
    let odir = PathBuf::from(rust_odir);

    let _ = clients::file::receive::receive_files(&config, &odir);
}
