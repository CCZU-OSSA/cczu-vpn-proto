use std::ffi::{c_char, c_int, c_uint, CStr, CString};

use cczuni::impls::services::webvpn::WebVPNService;

use crate::proxy::service;

pub const VERSION: &CStr = c"v1.0.0";

#[no_mangle]
pub extern "C" fn version() -> *const c_char {
    VERSION.as_ptr()
}

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

/// Deserialize data with json, and remember to dealloc it.
#[no_mangle]
pub extern "C" fn proxy_server() -> *mut c_char {
    let guard = match service::PROXY_SERVER.read() {
        Ok(inner) => inner,
        Err(poisoned) => poisoned.into_inner(),
    };

    let inner = guard.as_ref();

    if let Some(data) = inner {
        return CString::new(serde_json::to_string(data).unwrap())
            .unwrap()
            .into_raw();
    }

    CString::default().into_raw()
}

/// After call this, remember to dealloc your data...
#[no_mangle]
pub extern "C" fn send_packet(packet: *const c_uint, size: c_int) {
    todo!()
}

/// When you use up the data, please dealloc this.
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

#[no_mangle]
pub extern "C" fn free_memory(pointer: *mut c_char) {
    unsafe {
        if pointer.is_null() {
            return;
        }
        let _ = CString::from_raw(pointer);
    };
}
