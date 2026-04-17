uniffi::setup_scaffolding!("cczuvpnproto");

pub mod auth;
pub mod bindings;
pub mod types;
pub mod vpn;

#[cfg(test)]
mod test {
    use crate::bindings;
    use crate::vpn::service::{self, start_service};

    #[test]
    fn test_bindings_version() {
        assert_eq!(bindings::version(), concat!("v", env!("CARGO_PKG_VERSION")));
    }

    #[tokio::test]
    async fn test() {
        let _ = start_service("", "").await;
        let guard = match service::PROXY_SERVER.read() {
            Ok(inner) => inner,
            Err(poisoned) => poisoned.into_inner(),
        };
        println!("available: {}", service::service_available().await);
        println!(
            "{}",
            serde_json::to_string(guard.as_ref().unwrap()).unwrap()
        );
        service::send_tcp_packet(&[0]).await;
    }
}
