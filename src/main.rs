use std::{
    fs::exists,
    io::{stdin, BufRead},
};

use cczuni::impls::services::webvpn::WebVPNService;
use cczuvpnproto::proxy::service::{self, send_tcp_packet};

#[cfg(target_os = "windows")]
async fn windows_implements() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    use std::{net::IpAddr, str::FromStr, sync::Arc};

    let guard = match service::PROXY_SERVER.read() {
        Ok(inner) => inner,
        Err(poisoned) => poisoned.into_inner(),
    };
    let info = guard.as_ref();
    if info.is_none() {
        panic!("No server to create TUN Device");
    }
    let server = info.unwrap().clone();
    println!("server {}", serde_json::to_string(&server).unwrap());
    let mut config = tun::Configuration::default();
    config
        .address(server.address)
        .netmask(server.mask)
        .tun_name("CCZU-VPN-PROTO")
        .up();
    config.platform_config(|config| {
        config.dns_servers(&[IpAddr::from_str(&server.dns).unwrap()]);
    });
    let device = Arc::new(tun::create(&config)?);
    let device_output = device.clone();

    service::start_polling_packet(move |a, b| {
        println!("rev datasize: {a}");
        device_output.send(&b).unwrap();
    });

    let mut buf = [0; 65535];
    loop {
        let len = device.recv(&mut buf).unwrap();
        //println!("                 send buf {:?}", &buf[..len]);
        println!("send datasize {len}");
        if !send_tcp_packet(&mut buf[..len]).await {
            println!("packet send failed");
        }
    }
}

#[tokio::main]
async fn main() {
    if !cczuni::impls::client::DefaultClient::default()
        .webvpn_available()
        .await
    {
        panic!("webvpn not available");
    }

    println!("用户: ");
    let mut user = String::new();
    stdin().lock().read_line(&mut user).unwrap();
    println!("密码: ");
    let mut password = String::new();
    stdin().lock().read_line(&mut password).unwrap();
    let status = service::start_service(user.trim(), password.trim()).await;
    if !status {
        panic!("Failed to login");
    }

    if cfg!(target_os = "windows") {
        use std::{fs::File, io::Write};
        let dll = include_bytes!("../wintun.dll");
        if !exists("wintun.dll").unwrap_or(false) {
            println!("Create wintun.dll...");
            let mut out = File::create("wintun.dll").unwrap();
            out.write(dll).unwrap();
        }

        windows_implements().await.unwrap();
    } else {
        panic!("This platform is not implemented yet...")
    }
}
