use serde::Serialize;

#[derive(Debug, Serialize, Clone, uniffi::Record)]
pub struct ProxyServer {
    pub address: String,
    pub mask: String,
    pub gateway: String,
    pub dns: String,
    pub wins: String,
    pub split_tunnel_routes: Vec<String>,
}

#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct StartOptions {
    pub no_verification: bool,
}

impl Default for StartOptions {
    fn default() -> Self {
        Self {
            no_verification: true,
        }
    }
}
