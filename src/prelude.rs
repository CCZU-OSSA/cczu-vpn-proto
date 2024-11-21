use std::ffi::{c_char, c_int, c_uint, CStr};

use cczuni::impls::services::webvpn::WebVPNService;

use crate::proxy::service;

#[no_mangle]
pub static VERSION: &[u8] = c"v1.0.0".to_bytes_with_nul();

/// if success, return true.
#[no_mangle]
pub extern "C" fn start_service(user: *const c_char, password: *const c_char) -> bool {
    let user = unsafe { CStr::from_ptr(user) }
        .to_string_lossy()
        .to_string();
    let password = unsafe { CStr::from_ptr(password) }
        .to_string_lossy()
        .to_string();
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(service::start_service(user, password))
}

/// maybe need size and packet
#[no_mangle]
pub extern "C" fn send_packet(packet: *const c_uint, size: c_int) {
    todo!()
}

/// no sure should use cint/cll
#[no_mangle]
pub extern "C" fn receive_packet(size: c_int) -> *const c_uint {
    todo!()
}

#[no_mangle]
pub extern "C" fn service_available() -> bool {
    service::service_available()
}

#[no_mangle]
pub extern "C" fn stop_service() -> bool {
    service::stop_service()
}

#[no_mangle]
pub extern "C" fn webvpn_available() -> bool {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(cczuni::impls::client::DefaultClient::default().webvpn_available())
}
