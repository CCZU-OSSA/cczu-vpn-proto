use std::{fmt::Debug, sync::Arc};

use cczuni::impls::services::webvpn::WebVPNService;

use crate::{types::ProxyServer, vpn::service};

pub const VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));

#[uniffi::export(with_foreign)]
pub trait PacketCallback: Send + Sync + Debug {
    fn on_packet(&self, size: u32, packet: Vec<u8>);
}

#[uniffi::export]
pub fn version() -> String {
    VERSION.to_string()
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn start_service(user: String, password: String) -> bool {
    service::start_service(user, password).await
}

#[uniffi::export]
pub fn proxy_server() -> Option<ProxyServer> {
    let guard = match service::PROXY_SERVER.read() {
        Ok(inner) => inner,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.as_ref().cloned()
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn send_packet(packet: Vec<u8>) -> bool {
    service::send_packet(packet.as_slice()).await
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn send_tcp_packet(packet: Vec<u8>) -> bool {
    service::send_tcp_packet(packet.as_slice()).await
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn send_heartbeat() -> bool {
    service::send_heartbeat().await
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn receive_packet(size: u32) -> Vec<u8> {
    service::receive_packet(size).await
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn service_available() -> bool {
    service::service_available().await
}

#[uniffi::export]
pub fn stop_service() -> bool {
    service::stop_service()
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn webvpn_available() -> bool {
    cczuni::impls::client::DefaultClient::default()
        .webvpn_available()
        .await
}

#[uniffi::export]
pub fn start_packet_polling(callback: Arc<dyn PacketCallback>) {
    service::start_polling_packet(move |size, packet| {
        callback.on_packet(size, packet);
    });
}

#[uniffi::export]
pub fn stop_packet_polling() {
    service::stop_polling_packet();
}
