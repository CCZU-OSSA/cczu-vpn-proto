use std::{
    io::{stdin, BufRead},
    sync::Arc,
};

use anyhow::{bail, Context, Result};
use cczuni::impls::services::webvpn::WebVPNService;
use cczuvpnproto::{diag, vpn::service};
use tracing::{debug, error, info, warn};
use tun_rs::DeviceBuilder;

#[cfg(target_os = "windows")]
fn ensure_wintun_dll() -> std::io::Result<()> {
    let dll_path = std::path::Path::new("wintun.dll");
    if !dll_path.exists() {
        info!(path = %dll_path.display(), "creating wintun dll");
        std::fs::write(dll_path, include_bytes!("../wintun.dll"))?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct WindowsRoute {
    destination: std::net::Ipv4Addr,
    netmask: std::net::Ipv4Addr,
}

#[cfg(target_os = "windows")]
fn prefix_to_netmask(prefix: u8) -> Result<std::net::Ipv4Addr> {
    if prefix > 32 {
        bail!("invalid IPv4 prefix length: {prefix}");
    }
    let bits = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - u32::from(prefix))
    };
    Ok(std::net::Ipv4Addr::from(bits))
}

#[cfg(target_os = "windows")]
fn normalize_network(
    address: std::net::Ipv4Addr,
    netmask: std::net::Ipv4Addr,
) -> std::net::Ipv4Addr {
    std::net::Ipv4Addr::from(u32::from(address) & u32::from(netmask))
}

#[cfg(target_os = "windows")]
fn parse_split_tunnel_route(route: &str) -> Result<WindowsRoute> {
    let trimmed = route.trim();
    if trimmed.is_empty() {
        bail!("received an empty split-tunnel route");
    }

    let (address, netmask) = match trimmed.split_once('/') {
        Some((address, suffix)) => {
            let address = address
                .parse::<std::net::Ipv4Addr>()
                .with_context(|| format!("invalid split-tunnel IPv4 address: {trimmed}"))?;
            let netmask = match suffix.parse::<u8>() {
                Ok(prefix) => prefix_to_netmask(prefix)?,
                Err(_) => suffix.parse::<std::net::Ipv4Addr>().with_context(|| {
                    format!("invalid split-tunnel IPv4 netmask suffix: {trimmed}")
                })?,
            };
            (address, netmask)
        }
        None => (
            trimmed
                .parse::<std::net::Ipv4Addr>()
                .with_context(|| format!("invalid split-tunnel IPv4 host route: {trimmed}"))?,
            std::net::Ipv4Addr::new(255, 255, 255, 255),
        ),
    };

    Ok(WindowsRoute {
        destination: normalize_network(address, netmask),
        netmask,
    })
}

#[cfg(target_os = "windows")]
fn run_route_command(args: &[String]) -> Result<()> {
    let output = std::process::Command::new("route")
        .args(args)
        .output()
        .with_context(|| format!("failed to run route command: {:?}", args))?;
    if output.status.success() {
        return Ok(());
    }

    bail!(
        "route command failed: args={:?}, stdout={}, stderr={}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(target_os = "windows")]
fn add_split_tunnel_route(
    route: WindowsRoute,
    gateway: std::net::Ipv4Addr,
    if_index: u32,
) -> Result<()> {
    run_route_command(&[
        String::from("ADD"),
        route.destination.to_string(),
        String::from("MASK"),
        route.netmask.to_string(),
        gateway.to_string(),
        String::from("IF"),
        if_index.to_string(),
    ])
    .with_context(|| {
        format!(
            "failed to add split-tunnel route {}/{} via {} on interface {}",
            route.destination, route.netmask, gateway, if_index
        )
    })?;
    info!(
        destination = %route.destination,
        netmask = %route.netmask,
        gateway = %gateway,
        if_index,
        "installed split-tunnel route"
    );
    Ok(())
}

#[cfg(target_os = "windows")]
fn delete_split_tunnel_route(
    route: WindowsRoute,
    gateway: std::net::Ipv4Addr,
    if_index: u32,
) -> Result<()> {
    run_route_command(&[
        String::from("DELETE"),
        route.destination.to_string(),
        String::from("MASK"),
        route.netmask.to_string(),
        gateway.to_string(),
        String::from("IF"),
        if_index.to_string(),
    ])
    .with_context(|| {
        format!(
            "failed to delete split-tunnel route {}/{} via {} on interface {}",
            route.destination, route.netmask, gateway, if_index
        )
    })?;
    info!(
        destination = %route.destination,
        netmask = %route.netmask,
        gateway = %gateway,
        if_index,
        "removed split-tunnel route"
    );
    Ok(())
}

#[cfg(target_os = "windows")]
fn configure_split_tunnel_routes(
    device: &tun_rs::SyncDevice,
    server: &cczuvpnproto::types::ProxyServer,
) -> Result<Vec<WindowsRoute>> {
    let gateway = server
        .gateway
        .parse::<std::net::Ipv4Addr>()
        .with_context(|| format!("invalid VPN gateway address: {}", server.gateway))?;
    let if_index = device
        .if_index()
        .context("failed to query TUN interface index")?;

    let local_route = WindowsRoute {
        destination: normalize_network(
            server
                .address
                .parse::<std::net::Ipv4Addr>()
                .with_context(|| format!("invalid VPN address: {}", server.address))?,
            server
                .mask
                .parse::<std::net::Ipv4Addr>()
                .with_context(|| format!("invalid VPN mask: {}", server.mask))?,
        ),
        netmask: server
            .mask
            .parse::<std::net::Ipv4Addr>()
            .with_context(|| format!("invalid VPN mask: {}", server.mask))?,
    };

    let mut routes: Vec<WindowsRoute> = server
        .split_tunnel_routes
        .iter()
        .map(|route| parse_split_tunnel_route(route))
        .collect::<Result<Vec<WindowsRoute>>>()?;
    routes.retain(|route| route != &local_route);
    routes.sort();
    routes.dedup();

    let mut installed_routes = Vec::new();
    for route in &routes {
        if let Err(err) = add_split_tunnel_route(*route, gateway, if_index) {
            for installed_route in installed_routes.iter().rev().copied() {
                let _ = delete_split_tunnel_route(installed_route, gateway, if_index);
            }
            return Err(err);
        }
        installed_routes.push(*route);
    }

    Ok(routes)
}

#[cfg(target_os = "windows")]
fn cleanup_split_tunnel_routes(
    device: &tun_rs::SyncDevice,
    server: &cczuvpnproto::types::ProxyServer,
    routes: &[WindowsRoute],
) -> Result<()> {
    let gateway = server
        .gateway
        .parse::<std::net::Ipv4Addr>()
        .with_context(|| format!("invalid VPN gateway address: {}", server.gateway))?;
    let if_index = device
        .if_index()
        .context("failed to query TUN interface index")?;

    for route in routes {
        delete_split_tunnel_route(*route, gateway, if_index)?;
    }

    Ok(())
}

async fn create_device() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        ensure_wintun_dll()?;
    }

    let server = service::proxy_server()
        .ok_or_else(|| std::io::Error::other("No server to create TUN device"))?;
    info!(?server, "creating tun device");

    #[cfg(target_os = "windows")]
    let destination = Some(server.gateway.as_str());
    #[cfg(not(target_os = "windows"))]
    let destination: Option<&str> = None;

    let mut builder = DeviceBuilder::new().name("CCZU-VPN-PROTO").ipv4(
        server.address.as_str(),
        server.mask.as_str(),
        destination,
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

    #[cfg(target_os = "windows")]
    let split_tunnel_routes = configure_split_tunnel_routes(&device, &server)?;

    let device = Arc::new(device);
    let device_output = device.clone();

    service::start_polling_packet(move |a, b| {
        debug!(packet_size = a, "received packet from proxy");
        if let Err(err) = device_output.send(&b) {
            error!(packet_size = a, error = %err, "failed to write packet to TUN device");
        }
    })?;

    let loop_result: Result<()> = async {
        let mut buf = [0; 65535];
        loop {
            let len = device.recv(&mut buf)?;
            debug!(packet_size = len, "read packet from tun device");
            if let Err(err) = service::send_tcp_packet(&buf[..len]).await {
                error!(packet_size = len, error = %err, "failed to send packet to proxy");
            }

            if service::POLLER_SIGNAL.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(());
            }
        }
    }
    .await;

    #[cfg(target_os = "windows")]
    let cleanup_result =
        cleanup_split_tunnel_routes(device.as_ref(), &server, &split_tunnel_routes);

    loop_result?;

    #[cfg(target_os = "windows")]
    cleanup_result?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    diag::try_init_tracing("info");

    if !cczuni::impls::client::DefaultClient::default()
        .webvpn_available()
        .await
    {
        let mut choice = String::new();
        warn!("webvpn availability check failed, asking user whether to continue");
        println!("webvpn may not be available, are you sure to connect? (Y/n)");
        stdin().lock().read_line(&mut choice).unwrap();

        if choice.trim().to_lowercase() == "n" {
            return Ok(());
        }
    }

    println!("用户: ");
    let mut user = String::new();
    stdin().lock().read_line(&mut user).unwrap();
    println!("密码: ");
    let mut password = String::new();
    stdin().lock().read_line(&mut password).unwrap();
    info!("starting interactive vpn session");
    service::start_service(user.trim(), password.trim()).await?;
    create_device().await
}
