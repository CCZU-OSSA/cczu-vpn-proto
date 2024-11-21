use std::sync::{Arc, Mutex};

use tokio::{io::AsyncWriteExt, net::TcpStream};
use tokio_rustls::{client, rustls::ClientConfig, TlsConnector};

use crate::cczu::authorize;

use super::{
    proto::{AuthorizationPacket, Packet},
    trust::NoVerification,
};

pub static PROXY: Mutex<Option<client::TlsStream<TcpStream>>> = Mutex::new(None);

/// true -> ok
/// false -> failed
pub async fn start_service(user: impl Into<String>, password: impl Into<String>) -> bool {
    let mut guard = match PROXY.lock() {
        Ok(inner) => inner,
        Err(poisoned) => poisoned.into_inner(),
    };

    // Service is Running...
    if guard.is_some() {
        return false;
    }

    if let Ok(data) = authorize(user, password).await {
        let config = Arc::new(
            ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(NoVerification {}))
                .with_no_client_auth(),
        );
        let addr = format!("{}:{}", data.data.server, data.data.admin_port); // TODO Check Me

        let connector = TlsConnector::from(config);
        let tcpstream = TcpStream::connect(addr).await.unwrap();
        let mut io = connector
            .connect(data.data.server.try_into().unwrap(), tcpstream) // TODO Check Me
            .await
            .unwrap();

        io.write(
            AuthorizationPacket::new(data.data.token, "...".to_string())
                .build()
                .as_slice(),
        )
        .await
        .unwrap();
        // TODO
        guard.replace(io);
        return true;
    }

    false
}

pub fn service_available() -> bool {
    let guard = match PROXY.lock() {
        Ok(inner) => inner,
        Err(poisoned) => poisoned.into_inner(),
    };

    return guard.is_some();
}

/// false -> guard is none
pub fn stop_service() -> bool {
    let mut guard = match PROXY.lock() {
        Ok(inner) => inner,
        Err(poisoned) => poisoned.into_inner(),
    };

    if guard.is_none() {
        false
    } else {
        let inner = guard.take().unwrap();
        drop(inner);
        true
    }
}
