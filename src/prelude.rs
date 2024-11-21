use std::ffi::{c_char, c_int};

use cczuni::impls::services::webvpn::WebVPNService;

/// if success, return true.
#[no_mangle]
pub extern "C" fn start_service(user: *const c_char, password: *const c_char) -> bool {
    false
}

/// maybe need size and packet
#[no_mangle]
pub extern "C" fn send_packet() {
    todo!()
}

/// no sure should use cint/cll
#[no_mangle]
pub extern "C" fn receive_packet() {
    todo!()
}

#[no_mangle]
pub extern "C" fn service_available() -> bool {
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
