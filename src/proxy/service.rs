use std::sync::{Arc, Mutex, RwLock};

use tokio::{io::AsyncWriteExt, net::TcpStream};
use tokio_rustls::{client, rustls::ClientConfig, TlsConnector};

use crate::{cczu::authorize, ffi::ProxyServer};

use super::{
    proto::{
        read::comsume_authization,
        write::{AuthorizationPacket, Packet},
    },
    trust::NoVerification,
};

pub static PROXY: Mutex<Option<client::TlsStream<TcpStream>>> = Mutex::new(None);
pub static PROXY_SERVER: RwLock<Option<ProxyServer>> = RwLock::new(None);

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

        guard.replace(io);
        // Release Mutex here for comsume later...
        drop(guard);
        if let Ok(data) = comsume_authization().await {
            let mut guard = match PROXY_SERVER.write() {
                Ok(inner) => inner,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.replace(data);
            return true;
        }
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
        let mut psguard = match PROXY_SERVER.write() {
            Ok(inner) => inner,
            Err(poisoned) => poisoned.into_inner(),
        };
        drop(psguard.take().unwrap());
        drop(guard.take().unwrap());
        true
    }
}
