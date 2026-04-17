uniffi::setup_scaffolding!("cczuvpnproto");

pub mod auth;
pub mod bindings;
pub mod diag;
pub mod types;
pub mod vpn;

#[cfg(test)]
mod test {
    use crate::vpn::service::{self, start_service};
    use crate::{bindings, diag};
    use tracing::info;

    #[test]
    fn test_bindings_version() {
        assert_eq!(bindings::version(), concat!("v", env!("CARGO_PKG_VERSION")));
    }

    #[tokio::test]
    async fn test() {
        diag::try_init_tracing("info");
        let _ = start_service("", "").await;
        info!(
            available = service::service_available().await,
            "service availability"
        );
        info!(server = ?service::proxy_server(), "proxy server snapshot");
        let _ = service::send_tcp_packet(&[0]).await;
    }
}
