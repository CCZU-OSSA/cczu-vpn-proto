use std::sync::{Arc, Mutex, RwLock};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_rustls::{client, rustls::ClientConfig, TlsConnector};

use crate::{cczu::authorize, ffi::ProxyServer};

use super::{
    proto::{
        read::comsume_authization,
        write::{AuthorizationPacket, Packet, TCPPacket, HEARTBEAT},
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
    let user: String = user.into();
    let authorization = authorize(user.clone(), password).await;
    if let Ok(data) = authorization {
        let config = Arc::new(
            ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(NoVerification {}))
                .with_no_client_auth(),
        );
        let addr = "zmvpn.cczu.edu.cn:443";
        let connector = TlsConnector::from(config);
        let tcpstream = TcpStream::connect(addr).await.unwrap();
        let mut io = connector
            .connect("zmvpn.cczu.edu.cn".try_into().unwrap(), tcpstream) // TODO Check Me
            .await
            .unwrap();

        io.write(
            AuthorizationPacket::new(data.data.token, user.clone())
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

pub async fn receive_packet(size: u32) -> Vec<u8> {
    let mut guard = match PROXY.lock() {
        Ok(inner) => inner,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(proxy) = guard.as_mut() {
        let mut data = vec![0u8; size as usize];

        if let Ok(got) = proxy.read_exact(&mut data).await {
            let mut real = vec![];
            real.write_u32(got as u32).await.unwrap();
            real.write_all(data.as_slice()).await.unwrap();
            return real;
        }
    }

    return vec![0, 0, 0, 0];
}

pub async fn send_packet(packet: &[u8]) -> bool {
    let mut guard = match PROXY.lock() {
        Ok(inner) => inner,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(proxy) = guard.as_mut() {
        proxy.write(packet).await.unwrap();
        return true;
    }

    return false;
}

pub async fn send_tcp_packet(packet: &[u8]) -> bool {
    let mut guard = match PROXY.lock() {
        Ok(inner) => inner,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(proxy) = guard.as_mut() {
        proxy
            .write(TCPPacket::new(packet).build().as_slice())
            .await
            .unwrap();
        return true;
    }

    return false;
}

pub async fn send_heartbeat() -> bool {
    let mut guard = match PROXY.lock() {
        Ok(inner) => inner,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(proxy) = guard.as_mut() {
        proxy.write(&HEARTBEAT).await.unwrap();
        return true;
    }

    return false;
}
