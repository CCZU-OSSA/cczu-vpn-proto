use cczuvpnproto::proxy::service::{self, send_heartbeat};

#[tokio::main]
async fn main() {
    let _ = service::start_service("", "").await;
    let guard = match service::PROXY_SERVER.read() {
        Ok(inner) => inner,
        Err(poisoned) => poisoned.into_inner(),
    };
    println!("available: {}", service::service_available().await);
    println!(
        "{}",
        serde_json::to_string(guard.as_ref().unwrap()).unwrap()
    );
    send_heartbeat().await;

    service::start_polling_packet(|a, b| {
        println!("rev datasize: {a}, {b:?}");
    });
    service::waiting_polling_packet_stop().await.unwrap();
}
