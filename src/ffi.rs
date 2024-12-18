use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct ProxyServer {
    pub address: String,
    pub mask: String,
    pub gateway: String,
    pub dns: String,
    pub wins: String,
}
