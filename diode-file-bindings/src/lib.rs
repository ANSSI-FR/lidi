#![allow(unsafe_code)]

use diode::file;
use std::{
    ffi::{c_char, CStr},
    net::SocketAddr,
    path::PathBuf,
    ptr,
    str::FromStr,
};

#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn diode_new_config(
    ptr_addr: *const c_char,
    buffer_size: u32,
) -> *mut file::Config {
    if ptr_addr.is_null() {
        return ptr::null_mut();
    }
    let cstr_addr = unsafe { CStr::from_ptr(ptr_addr) };
    let rust_addr = String::from_utf8_lossy(cstr_addr.to_bytes()).to_string();
    let socket_addr = SocketAddr::from_str(&rust_addr).expect("ip:port");

    let config = Box::new(file::Config {
        socket_addr,
        buffer_size: buffer_size as usize,
    });
    Box::into_raw(config)
}

#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn diode_free_config(ptr: *mut file::Config) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(ptr));
    }
}

#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn diode_send_file(
    ptr: *mut file::Config,
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

    file::send::send_file(config, &rust_filepath).unwrap_or(0) as u32
}

#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn diode_receive_files(ptr: *mut file::Config, ptr_odir: *const c_char) {
    if ptr.is_null() {
        return;
    }
    let config = unsafe { ptr.as_ref() }.expect("config");

    if ptr_odir.is_null() {
        return;
    }
    let cstr_odir = unsafe { CStr::from_ptr(ptr_odir) };
    let rust_odir = String::from_utf8_lossy(cstr_odir.to_bytes()).to_string();
    let odir = PathBuf::from(rust_odir);

    let _ = file::receive::receive_files(config.clone(), odir);
}
