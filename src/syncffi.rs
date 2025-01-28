// !DONT CALL THEM IN A TOKIO ASYNC CONTEXT!
use std::{
    ffi::{c_char, c_uchar, c_uint, CStr, CString},
    future::Future,
    slice,
    sync::LazyLock,
};

use cczuni::impls::services::webvpn::WebVPNService;
use tokio::runtime::Runtime;

use crate::proxy::service;

pub const VERSION: &CStr = c"v1.0.0+1";

#[no_mangle]
pub extern "C" fn version() -> *const c_char {
    VERSION.as_ptr()
}

pub static RT: LazyLock<Runtime> =
    LazyLock::new(|| Runtime::new().expect("Create Tokio Runtime failed!"));

fn run_sync_in_rt<F: Future>(f: F) -> F::Output {
    RT.block_on(f)
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
    run_sync_in_rt(service::start_service(user, password))
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
pub extern "C" fn send_packet(packet: *const c_uchar, size: c_uint) -> bool {
    let bytes = unsafe { slice::from_raw_parts(packet, size as usize) };
    run_sync_in_rt(service::send_packet(bytes))
}

#[no_mangle]
pub extern "C" fn send_tcp_packet(packet: *const c_uchar, size: c_uint) -> bool {
    let bytes = unsafe { slice::from_raw_parts(packet, size as usize) };
    run_sync_in_rt(service::send_tcp_packet(bytes))
}

#[no_mangle]
pub extern "C" fn send_heartbeat() -> bool {
    run_sync_in_rt(service::send_heartbeat())
}

/// When you use up the data, please dealloc this.
#[no_mangle]
pub extern "C" fn receive_packet(size: c_uint) -> *mut c_uchar {
    run_sync_in_rt(service::receive_packet(size)).as_mut_ptr()
}

#[no_mangle]
pub extern "C" fn service_available() -> bool {
    run_sync_in_rt(service::service_available())
}

#[no_mangle]
pub extern "C" fn stop_service() -> bool {
    service::stop_service()
}

#[no_mangle]
pub extern "C" fn webvpn_available() -> bool {
    run_sync_in_rt(cczuni::impls::client::DefaultClient::default().webvpn_available())
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

#[no_mangle]
pub extern "C" fn start_polling_packet(
    callback: extern "C" fn(size: c_uint, packet: *mut c_uchar),
) {
    service::start_polling_packet(move |size, mut data| {
        callback(size, data.as_mut_ptr());
    });
}

#[no_mangle]
pub extern "C" fn stop_polling_packet() {
    service::stop_polling_packet();
}
