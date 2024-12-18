mod cczu;
mod ffi;
pub mod prelude;
mod proxy;

#[cfg(test)]
mod test {
    use crate::proxy::service::{self, start_service};

    #[tokio::test]
    async fn test() {
        let _ = start_service("", "").await;
        let guard = match service::PROXY_SERVER.read() {
            Ok(inner) => inner,
            Err(poisoned) => poisoned.into_inner(),
        };
        println!(
            "{}",
            serde_json::to_string(guard.as_ref().unwrap()).unwrap()
        );
    }
}
