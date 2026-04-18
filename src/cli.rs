use std::{
    io::{BufRead, Write, stdin, stdout},
    sync::Arc,
};

use anyhow::{Context, Result, bail};
use cczuni::impls::services::webvpn::WebVPNService;
use cczuvpnproto::{diag, vpn::service};
use clap::Parser;
use rpassword::prompt_password;
use tracing::{debug, error, info, warn};
use tun_rs::{DeviceBuilder, InterruptEvent};

#[derive(Debug, Parser)]
#[command(name = "cczu-vpn-proto")]
#[command(about = "CCZU WebVPN CLI client")]
struct CliArgs {
    #[arg(long)]
    user: Option<String>,
    #[arg(long)]
    password: Option<String>,
    #[arg(long, default_value = "info")]
    log_level: String,
    #[arg(long)]
    yes: bool,
}

fn read_prompt(prompt: &str) -> Result<String> {
    print!("{prompt}");
    stdout().flush().context("failed to flush stdout prompt")?;

    let mut value = String::new();
    stdin()
        .lock()
        .read_line(&mut value)
        .context("failed to read console input")?;

    Ok(value.trim().to_string())
}

fn read_required_prompt(prompt: &str) -> Result<String> {
    loop {
        let value = read_prompt(prompt)?;
        if !value.is_empty() {
            return Ok(value);
        }
        warn!(prompt, "received empty input, asking again");
    }
}

fn read_password_prompt(prompt: &str) -> Result<String> {
    prompt_password(prompt).context("failed to read password input")
}

fn read_required_password_prompt(prompt: &str) -> Result<String> {
    loop {
        let value = read_password_prompt(prompt)?;
        if !value.is_empty() {
            return Ok(value);
        }
        warn!(prompt, "received empty password, asking again");
    }
}

fn resolve_credentials(args: &CliArgs) -> Result<(String, String)> {
    let user = match &args.user {
        Some(user) if !user.trim().is_empty() => user.trim().to_string(),
        _ => read_required_prompt("用户: ")?,
    };

    let password = match &args.password {
        Some(password) if !password.trim().is_empty() => password.trim().to_string(),
        _ => read_required_password_prompt("密码（输入已隐藏）: ")?,
    };

    Ok((user, password))
}

fn confirm_continue(prompt: &str) -> Result<bool> {
    let choice = read_prompt(prompt)?;
    Ok(choice.trim().to_lowercase() != "n")
}

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
fn parse_split_tunnel_route(route: &str) -> Result<Option<WindowsRoute>> {
    let trimmed = route.trim();
    if trimmed.is_empty() {
        bail!("received an empty split-tunnel route");
    }

    let route_target = if trimmed.contains(';') {
        let mut parts = trimmed.split(';');
        let _protocol = parts
            .next()
            .with_context(|| format!("invalid split-tunnel rule format: {trimmed}"))?;
        let target = parts
            .next()
            .with_context(|| format!("invalid split-tunnel rule format: {trimmed}"))?;
        let _port = parts
            .next()
            .with_context(|| format!("invalid split-tunnel rule format: {trimmed}"))?;
        if parts.next().is_some() {
            bail!("invalid split-tunnel rule format: {trimmed}");
        }
        target.trim()
    } else {
        trimmed
    };

    let (address, netmask) = match route_target.split_once('/') {
        Some((address, suffix)) => {
            let address = match address.parse::<std::net::IpAddr>() {
                Ok(std::net::IpAddr::V4(address)) => address,
                Ok(std::net::IpAddr::V6(_)) => return Ok(None),
                Err(_) => {
                    bail!("invalid split-tunnel IP address: {route_target}");
                }
            };
            let netmask = match suffix.parse::<u8>() {
                Ok(prefix) => prefix_to_netmask(prefix)?,
                Err(_) => suffix.parse::<std::net::Ipv4Addr>().with_context(|| {
                    format!("invalid split-tunnel IPv4 netmask suffix: {route_target}")
                })?,
            };
            (address, netmask)
        }
        None => {
            let address = match route_target.parse::<std::net::IpAddr>() {
                Ok(std::net::IpAddr::V4(address)) => address,
                Ok(std::net::IpAddr::V6(_)) => return Ok(None),
                Err(_) => {
                    bail!("invalid split-tunnel IP host route: {route_target}");
                }
            };
            (address, std::net::Ipv4Addr::new(255, 255, 255, 255))
        }
    };

    Ok(Some(WindowsRoute {
        destination: normalize_network(address, netmask),
        netmask,
    }))
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

    let mut routes = Vec::new();
    let mut skipped_route_count = 0usize;
    for route in &server.split_tunnel_routes {
        match parse_split_tunnel_route(route)? {
            Some(route) => routes.push(route),
            None => {
                skipped_route_count += 1;
                warn!(
                    rule = route,
                    "skipping unsupported non-IPv4 split-tunnel rule"
                );
            }
        }
    }

    routes.retain(|route| route != &local_route);
    routes.sort();
    routes.dedup();

    if skipped_route_count > 0 {
        warn!(
            skipped_route_count,
            "skipped unsupported split-tunnel rules while configuring Windows routes"
        );
    }

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
    let interrupt_event = Arc::new(InterruptEvent::new()?);
    let shutdown_requested = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let ctrl_c_interrupt_event = interrupt_event.clone();
    let ctrl_c_shutdown_requested = shutdown_requested.clone();
    let ctrl_c_task = tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .context("failed to listen for ctrl+c")?;
        info!("received ctrl+c, starting shutdown");
        ctrl_c_shutdown_requested.store(true, std::sync::atomic::Ordering::Relaxed);
        service::stop_polling_packet();
        ctrl_c_interrupt_event
            .trigger()
            .context("failed to trigger tun interrupt event")?;
        Ok::<(), anyhow::Error>(())
    });

    service::start_polling_packet(move |a, b| {
        debug!(packet_size = a, "received packet from proxy");
        if let Err(err) = device_output.send(&b) {
            error!(packet_size = a, error = %err, "failed to write packet to TUN device");
        }
    })?;

    let loop_result: Result<()> = async {
        let mut buf = [0; 65535];
        loop {
            let len = match device.recv_intr(&mut buf, interrupt_event.as_ref()) {
                Ok(len) => len,
                Err(err)
                    if err.kind() == std::io::ErrorKind::Interrupted
                        && shutdown_requested.load(std::sync::atomic::Ordering::Relaxed) =>
                {
                    info!("tun read interrupted for shutdown");
                    break;
                }
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {
                    warn!(error = %err, "tun read interrupted unexpectedly");
                    continue;
                }
                Err(err) => return Err(err.into()),
            };
            debug!(packet_size = len, "read packet from tun device");
            if let Err(err) = service::send_tcp_packet(&buf[..len]).await {
                error!(packet_size = len, error = %err, "failed to send packet to proxy");
            }

            if service::POLLER_SIGNAL.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(());
            }
        }
        Ok(())
    }
    .await;

    let stop_result = service::stop_service().await;

    #[cfg(target_os = "windows")]
    let cleanup_result =
        cleanup_split_tunnel_routes(device.as_ref(), &server, &split_tunnel_routes);

    let ctrl_c_result = if shutdown_requested.load(std::sync::atomic::Ordering::Relaxed) {
        ctrl_c_task.await.context("ctrl+c task join failed")?
    } else {
        ctrl_c_task.abort();
        Ok(())
    };

    loop_result?;
    stop_result?;
    #[cfg(target_os = "windows")]
    cleanup_result?;
    ctrl_c_result?;

    Ok(())
}

pub async fn run() -> Result<()> {
    let args = CliArgs::parse();
    diag::try_init_tracing(args.log_level.as_str());

    if !cczuni::impls::client::DefaultClient::default()
        .webvpn_available()
        .await
    {
        warn!("webvpn availability check failed, asking user whether to continue");
        if !args.yes
            && !confirm_continue("webvpn may not be available, are you sure to connect? (Y/n) ")?
        {
            return Ok(());
        }
    }

    let (user, password) = resolve_credentials(&args)?;
    info!("starting interactive vpn session");
    service::start_service(user, password).await?;
    create_device().await
}
