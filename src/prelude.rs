use std::ffi::{c_char, c_int};

use cczuni::impls::services::webvpn::WebVPNService;

/// If Success, return true.
#[no_mangle]
pub extern "C" fn start_service(user: *const c_char, password: *const c_char) -> bool {
    false
}

#[no_mangle]
pub extern "C" fn services_available() -> bool {
    false
}

#[no_mangle]
pub extern "C" fn stop_service() -> bool {
    false
}

#[no_mangle]
pub extern "C" fn version_major() -> c_int {
    1
}

#[no_mangle]
pub extern "C" fn version_minor() -> c_int {
    0
}

#[no_mangle]
pub extern "C" fn version_patch() -> c_int {
    0
}

#[no_mangle]
pub extern "C" fn webvpn_available() -> bool {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(cczuni::impls::client::DefaultClient::default().webvpn_available())
}
