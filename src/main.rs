use std::{
    io::{stdin, BufRead},
    sync::Arc,
};

use anyhow::Result;
use cczuni::impls::services::webvpn::WebVPNService;
use cczuvpnproto::vpn::service;
use tun_rs::DeviceBuilder;

#[cfg(target_os = "windows")]
fn ensure_wintun_dll() -> std::io::Result<()> {
    let dll_path = std::path::Path::new("wintun.dll");
    if !dll_path.exists() {
        println!("Create wintun.dll...");
        std::fs::write(dll_path, include_bytes!("../wintun.dll"))?;
    }
    Ok(())
}

async fn create_device() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        ensure_wintun_dll()?;
    }
    let guard = match service::PROXY_SERVER.read() {
        Ok(inner) => inner,
        Err(poisoned) => poisoned.into_inner(),
    };
    let server = guard
        .as_ref()
        .cloned()
        .ok_or_else(|| std::io::Error::other("No server to create TUN device"))?;
    println!("server {}", serde_json::to_string(&server)?);

    let mut builder = DeviceBuilder::new().name("CCZU-VPN-PROTO").ipv4(
        server.address.as_str(),
        server.mask.as_str(),
        None,
    );

    #[cfg(target_os = "windows")]
    {
        builder = builder
            .description("CCZU-VPN-PROTO")
            .wintun_file(String::from("wintun.dll"));
    }

    let device = builder.build_sync()?;

    #[cfg(target_os = "windows")]
    {
        let dns_server = server.dns.parse::<std::net::IpAddr>()?;
        device.set_dns_servers(&[dns_server])?;
    }

    let device = Arc::new(device);
    let device_output = device.clone();

    service::start_polling_packet(move |a, b| {
        println!("rev datasize: {a}");
        if let Err(err) = device_output.send(&b) {
            panic!("Failed to write packet to TUN device: {err}");
        }
    });

    let mut buf = [0; 65535];
    loop {
        let len = device.recv(&mut buf)?;
        println!("send datasize {len}");
        if !service::send_tcp_packet(&mut buf[..len]).await {
            println!("packet send failed");
        }

        if service::POLLER_SIGNAL.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(());
        }
    }
}

#[tokio::main]
async fn main() {
    if !cczuni::impls::client::DefaultClient::default()
        .webvpn_available()
        .await
    {
        let mut choice = String::new();
        println!("webvpn may not be available, are you sure to connect? (Y/n)");
        stdin().lock().read_line(&mut choice).unwrap();

        if choice.trim().to_lowercase() == "n" {
            return;
        }
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
    create_device().await.unwrap();
}
