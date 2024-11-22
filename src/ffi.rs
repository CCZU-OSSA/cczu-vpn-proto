use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct ProxyServer {
    pub address: [u8; 4],
    pub mask: [u8; 4],
    pub gateway: [u8; 4],
    pub dns: String,
    pub wins: String,
}
