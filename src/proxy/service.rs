use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, RwLock,
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::Mutex,
    task::JoinHandle,
};
use tokio_rustls::{client, rustls::ClientConfig, TlsConnector};

use crate::{cczu::authorize, model::ProxyServer, syncffi::RT};

use super::{
    proto::{
        read::{consume_authization, try_read_packet_data},
        write::{AuthorizationPacket, Packet, TCPPacket, HEARTBEAT},
    },
    trust::NoVerification,
};

pub static PROXY: Mutex<Option<client::TlsStream<TcpStream>>> = Mutex::const_new(None);
pub static PROXY_SERVER: RwLock<Option<ProxyServer>> = RwLock::new(None);
pub static POLLER: std::sync::Mutex<Option<JoinHandle<()>>> = std::sync::Mutex::new(None);
pub static POLLER_SIGNAL: AtomicBool = AtomicBool::new(false);

/// true -> ok
/// false -> failed
pub async fn start_service(user: impl Into<String>, password: impl Into<String>) -> bool {
    let mut guard = PROXY.lock().await;

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

        let realdns: String;
        if let Some(gateway) = data.data.gateway_list.first() {
            realdns = gateway.dns.clone();
        } else {
            return false;
        }

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
        // Release Mutex here for consume later...
        drop(guard);
        if let Ok(mut proxy) = consume_authization().await {
            proxy.dns = realdns;
            let mut guard = match PROXY_SERVER.write() {
                Ok(inner) => inner,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.replace(proxy);
            return true;
        }
    }

    false
}

pub async fn service_available() -> bool {
    return PROXY.lock().await.is_some();
}

/// false -> guard is none
pub fn stop_service() -> bool {
    let mut guard = PROXY.blocking_lock();

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
    let mut guard = PROXY.lock().await;

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
    let mut guard = PROXY.lock().await;

    if let Some(proxy) = guard.as_mut() {
        proxy.write(packet).await.unwrap();
        return true;
    }

    return false;
}

pub async fn send_tcp_packet(packet: &[u8]) -> bool {
    let mut guard = PROXY.lock().await;

    if let Some(proxy) = guard.as_mut() {
        if let Err(err) = proxy.write(TCPPacket::new(packet).build().as_slice()).await {
            println!("ERROR: SEND TCP PACKET - {}", err);
        }
        return true;
    }

    return false;
}

pub async fn send_heartbeat() -> bool {
    let mut guard = PROXY.lock().await;
    if let Some(proxy) = guard.as_mut() {
        proxy.write(&HEARTBEAT).await.unwrap();
        return true;
    }

    return false;
}

pub fn start_polling_packet(callback: impl Send + 'static + Fn(u32, Vec<u8>) -> ()) {
    let mut guard = match POLLER.lock() {
        Ok(inner) => inner,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(_) = guard.as_mut() {
        if !POLLER_SIGNAL.load(Ordering::Relaxed) {
            stop_service();
        }
    }

    let handler: JoinHandle<()> = tokio::runtime::Handle::try_current()
        .unwrap_or(RT.handle().clone())
        .spawn(async move {
            // waiting for available
            while POLLER_SIGNAL.load(Ordering::Relaxed) {}
            loop {
                // terminate
                if POLLER_SIGNAL.load(Ordering::Relaxed) {
                    break;
                }
                let op = try_read_packet_data().await;
                if let Ok(Some(data)) = op {
                    callback(data.len() as u32, data);
                } else if let Err(err) = op {
                    // DEBUG MAY NEED BREAK?
                    println!("ERROR: {err}");
                }
            }

            // restore
            POLLER_SIGNAL.store(false, Ordering::Relaxed);
        });

    guard.replace(handler);
}

pub fn stop_polling_packet() {
    POLLER_SIGNAL.store(true, Ordering::Relaxed);
}

pub async fn waiting_polling_packet_stop() -> Result<(), tokio::task::JoinError> {
    let mut guard = POLLER.lock().unwrap();
    if let Some(handler) = guard.as_mut() {
        return handler.await;
    }
    return Ok(());
}
