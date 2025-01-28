pub mod cczu;
pub mod model;
pub mod proxy;
pub mod syncffi;

#[cfg(test)]
mod test {
    use crate::proxy::service::{self, start_service};
    use crate::syncffi;

    #[test]
    fn test_syncffi() {
        println!("{}", syncffi::service_available());
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
