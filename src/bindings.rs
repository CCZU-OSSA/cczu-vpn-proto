use std::{fmt::Debug, sync::Arc};

use anyhow::Error;
use cczuni::impls::services::webvpn::WebVPNService;

use crate::{
    diag,
    types::{ProxyServer, StartOptions},
    vpn::service,
};

pub const VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));

#[derive(Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum BindingsError {
    Runtime(String),
}

impl std::fmt::Display for BindingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Runtime(message) => write!(f, "{message}"),
        }
    }
}

impl From<Error> for BindingsError {
    fn from(err: Error) -> Self {
        Self::Runtime(format!("{err:#}"))
    }
}

#[uniffi::export(with_foreign)]
pub trait PacketCallback: Send + Sync + Debug {
    fn on_packet(&self, size: u32, packet: Vec<u8>);
}

#[uniffi::export]
pub fn version() -> String {
    VERSION.to_string()
}

#[uniffi::export]
pub fn default_start_options() -> StartOptions {
    StartOptions::default()
}

#[uniffi::export]
pub fn init_tracing(level: String) -> Result<(), BindingsError> {
    diag::init_tracing(level.as_str()).map_err(Into::into)
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn start_service(user: String, password: String) -> Result<(), BindingsError> {
    service::start_service(user, password)
        .await
        .map_err(Into::into)
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn start_service_with_options(
    user: String,
    password: String,
    options: StartOptions,
) -> Result<(), BindingsError> {
    service::start_service_with_options(user, password, options)
        .await
        .map_err(Into::into)
}

#[uniffi::export]
pub fn proxy_server() -> Option<ProxyServer> {
    service::proxy_server()
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn send_packet(packet: Vec<u8>) -> Result<(), BindingsError> {
    service::send_packet(packet.as_slice())
        .await
        .map_err(Into::into)
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn send_tcp_packet(packet: Vec<u8>) -> Result<(), BindingsError> {
    service::send_tcp_packet(packet.as_slice())
        .await
        .map_err(Into::into)
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn send_heartbeat() -> Result<(), BindingsError> {
    service::send_heartbeat().await.map_err(Into::into)
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn receive_packet(size: u32) -> Result<Vec<u8>, BindingsError> {
    service::receive_packet(size).await.map_err(Into::into)
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn service_available() -> bool {
    service::service_available().await
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn stop_service() -> Result<(), BindingsError> {
    service::stop_service().await.map_err(Into::into)
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn webvpn_available() -> bool {
    cczuni::impls::client::DefaultClient::default()
        .webvpn_available()
        .await
}

#[uniffi::export]
pub fn start_packet_polling(callback: Arc<dyn PacketCallback>) -> Result<(), BindingsError> {
    service::start_polling_packet(move |size, packet| {
        callback.on_packet(size, packet);
    })
    .map_err(Into::into)
}

#[uniffi::export]
pub fn stop_packet_polling() {
    service::stop_polling_packet();
}
