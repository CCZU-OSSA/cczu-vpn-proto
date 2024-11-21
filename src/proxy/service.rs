use std::sync::Mutex;

use tokio::net::TcpStream;
use tokio_rustls::client;

use crate::cczu::authorize;

pub static PROXY: Mutex<Option<client::TlsStream<TcpStream>>> = Mutex::new(None);

pub async fn start_service(user: impl Into<String>, password: impl Into<String>) -> bool {
    if let Ok(data) = authorize(user, password).await {
        todo!()
    }

    false
}

pub fn service_available() -> bool {
    let locked = PROXY.lock();
    if let Ok(guard) = locked {
        let result = guard.is_some();
        drop(guard);
        return result;
    }

    false
}

pub fn stop_service() -> bool {
    let locked = PROXY.lock();
    if let Ok(mut guard) = locked {
        if guard.is_none() {
            return true;
        }

        let inner = guard.take().unwrap();
        drop(inner); // TODO May not stop it? Need test
        return true;
    }

    false
}
