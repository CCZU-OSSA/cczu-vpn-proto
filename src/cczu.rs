use cczuni::impls::{
    client::DefaultClient,
    login::sso::SSOUniversalLogin,
    services::{
        webvpn::WebVPNService,
        webvpn_type::{ElinkProxyData, Message},
    },
};

pub async fn authorize(
    user: impl Into<String>,
    password: impl Into<String>,
) -> Result<Message<ElinkProxyData>, tokio::io::Error> {
    let client = DefaultClient::account(user, password);
    if let Some(info) = client.sso_universal_login().await? {
        Ok(client.webvpn_get_proxy_service(info.userid).await?)
    } else {
        Err(tokio::io::Error::new(
            std::io::ErrorKind::Other,
            "WebVPN not avaiable",
        ))
    }
}
