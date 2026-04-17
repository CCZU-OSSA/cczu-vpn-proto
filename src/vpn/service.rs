use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex as StandardMutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, ensure, Context, Result};
use cczuni::impls::services::webvpn_type::ElinkProxyData;
use tokio::{
    io::{split, AsyncWriteExt, WriteHalf},
    net::TcpStream,
    sync::{broadcast, mpsc, oneshot, Mutex},
    task::JoinHandle as TokioJoinHandle,
    time,
};
use tokio_rustls::TlsConnector;
use tracing::{debug, info, warn};

use crate::{
    auth::authorize,
    types::{ProxyServer, StartOptions},
};

use super::{
    protocol::{
        read::consume_authization,
        stream::{InboundFrame, PacketStreamReader, ProxyStream},
        write::{AuthorizationPacket, Packet, TCPPacket, HEARTBEAT},
    },
    tls::build_client_config,
};

type ProxyWriteHalf = WriteHalf<ProxyStream>;
type PacketBroadcastSender = broadcast::Sender<Vec<u8>>;
type PacketQueueSender = mpsc::Sender<Vec<u8>>;
type PacketQueueReceiver = mpsc::Receiver<Vec<u8>>;
type PacketQueueHandle = Arc<Mutex<PacketQueueReceiver>>;
type StartCancelSignal = Arc<AtomicBool>;

pub static POLLER: StandardMutex<Option<JoinHandle<()>>> = StandardMutex::new(None);
pub static POLLER_SIGNAL: AtomicBool = AtomicBool::new(false);

static SESSION: StandardMutex<Option<SessionSlot>> = StandardMutex::new(None);
static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

const PACKET_QUEUE_CAPACITY: usize = 256;
const POLLER_BROADCAST_CAPACITY: usize = 256;

fn collect_split_tunnel_routes(proxy_data: &ElinkProxyData) -> Vec<String> {
    let mut routes = Vec::new();
    for gateway in &proxy_data.gateway_list {
        routes.extend(gateway.in_ip_list.iter().cloned());
        for route_group in gateway.in_ip_list_by_gateway_map.values() {
            routes.extend(route_group.iter().cloned());
        }
    }
    routes.sort();
    routes.dedup();
    routes
}

enum SessionSlot {
    Starting(StartingHandle),
    Running(SessionHandle),
}

struct StartingHandle {
    id: u64,
    cancel: StartCancelSignal,
}

struct SessionHandle {
    id: u64,
    server: ProxyServer,
    command_tx: mpsc::Sender<SessionCommand>,
    packet_rx: PacketQueueHandle,
    packet_tx: PacketBroadcastSender,
    task: TokioJoinHandle<Result<()>>,
}

enum SessionCommand {
    SendRaw {
        packet: Vec<u8>,
        response: oneshot::Sender<Result<()>>,
    },
    SendTcp {
        packet: Vec<u8>,
        response: oneshot::Sender<Result<()>>,
    },
    SendHeartbeat {
        response: oneshot::Sender<Result<()>>,
    },
    Stop,
}

fn lock_session() -> Result<std::sync::MutexGuard<'static, Option<SessionSlot>>> {
    SESSION
        .lock()
        .map_err(|_| anyhow!("session state lock poisoned"))
}

fn clone_command_tx() -> Result<mpsc::Sender<SessionCommand>> {
    let guard = lock_session()?;
    match guard.as_ref() {
        Some(SessionSlot::Running(handle)) => Ok(handle.command_tx.clone()),
        Some(SessionSlot::Starting(_)) => Err(anyhow!("service is still starting")),
        None => Err(anyhow!("service is not running")),
    }
}

fn clear_session_if_matches(session_id: u64) {
    if let Ok(mut guard) = SESSION.lock() {
        let should_clear = match guard.as_ref() {
            Some(SessionSlot::Starting(handle)) => handle.id == session_id,
            Some(SessionSlot::Running(handle)) => handle.id == session_id,
            None => false,
        };
        if should_clear {
            let _ = guard.take();
            debug!(session_id, "cleared session state");
        }
    }
}

fn check_start_cancelled(cancel: &StartCancelSignal) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "service start cancelled before initialization completed"
    );
    Ok(())
}

pub fn proxy_server() -> Option<ProxyServer> {
    let Ok(guard) = SESSION.lock() else {
        return None;
    };
    match guard.as_ref() {
        Some(SessionSlot::Running(handle)) => Some(handle.server.clone()),
        _ => None,
    }
}

async fn send_heartbeat_if_due(
    writer: &mut ProxyWriteHalf,
    last_sent: &mut Option<Instant>,
) -> Result<()> {
    let should_send = last_sent
        .map(|instant| instant.elapsed() >= Duration::from_secs(5))
        .unwrap_or(true);
    if should_send {
        writer
            .write_all(&HEARTBEAT)
            .await
            .context("failed to write heartbeat to proxy")?;
        last_sent.replace(Instant::now());
    }
    Ok(())
}

async fn run_session_actor(
    session_id: u64,
    mut reader: PacketStreamReader,
    mut writer: ProxyWriteHalf,
    mut command_rx: mpsc::Receiver<SessionCommand>,
    packet_queue_tx: PacketQueueSender,
    packet_tx: broadcast::Sender<Vec<u8>>,
) -> Result<()> {
    let mut last_heartbeat = None;
    let actor_result = async {
        loop {
            tokio::select! {
                biased;
                command = command_rx.recv() => {
                    match command {
                        Some(SessionCommand::SendRaw { packet, response }) => {
                            let result = writer
                                .write_all(&packet)
                                .await
                                .context("failed to write raw packet to proxy");
                            let _ = response.send(result);
                        }
                        Some(SessionCommand::SendTcp { packet, response }) => {
                            let result = async {
                                let built_packet = TCPPacket::new(&packet)
                                    .build()
                                    .context("failed to build TCP packet")?;
                                writer
                                    .write_all(built_packet.as_slice())
                                    .await
                                    .context("failed to write TCP packet to proxy")
                            }
                            .await;
                            let _ = response.send(result);
                        }
                        Some(SessionCommand::SendHeartbeat { response }) => {
                            let result = writer
                                .write_all(&HEARTBEAT)
                                .await
                                .context("failed to write heartbeat to proxy");
                            if result.is_ok() {
                                last_heartbeat.replace(Instant::now());
                            }
                            let _ = response.send(result);
                        }
                        Some(SessionCommand::Stop) | None => break,
                    }
                }
                frame = reader.try_read_frame() => {
                    match frame? {
                        Some(InboundFrame::Data { payload, .. }) => {
                            match packet_queue_tx.try_send(payload.clone()) {
                                Ok(()) => {}
                                Err(mpsc::error::TrySendError::Full(_)) => {
                                    warn!(
                                        session_id,
                                        packet_size = payload.len(),
                                        "packet queue is full, dropping inbound packet"
                                    );
                                }
                                Err(mpsc::error::TrySendError::Closed(_)) => {}
                            }
                            let _ = packet_tx.send(payload);
                        }
                        Some(InboundFrame::Control { .. }) | None => {
                            send_heartbeat_if_due(&mut writer, &mut last_heartbeat).await?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
    .await;

    clear_session_if_matches(session_id);
    actor_result
}

async fn request_command(
    command: SessionCommand,
    response_rx: oneshot::Receiver<Result<()>>,
) -> Result<()> {
    let command_tx = clone_command_tx()?;
    command_tx
        .send(command)
        .await
        .context("failed to send command to session actor")?;
    response_rx
        .await
        .context("session actor dropped command response")?
}

pub async fn start_service_with_options(
    user: impl Into<String>,
    password: impl Into<String>,
    options: StartOptions,
) -> Result<()> {
    let session_id = NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed);
    info!(
        session_id,
        no_verification = options.no_verification,
        "starting proxy service"
    );
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut guard = lock_session()?;
        if let Some(SessionSlot::Running(handle)) = guard.as_ref() {
            ensure!(!handle.task.is_finished(), "service is already running");
        }
        ensure!(guard.is_none(), "service is already running");
        guard.replace(SessionSlot::Starting(StartingHandle {
            id: session_id,
            cancel: cancel.clone(),
        }));
    }

    let start_result = async {
        let user: String = user.into();
        check_start_cancelled(&cancel)?;
        let authorization = authorize(user.clone(), password)
            .await
            .context("failed to authorize against webvpn")?;
        check_start_cancelled(&cancel)?;

        let realdns = authorization
            .data
            .gateway_list
            .first()
            .map(|gateway| gateway.dns.clone())
            .context("authorization response does not contain gateway dns")?;
        let split_tunnel_routes = collect_split_tunnel_routes(&authorization.data);

        let config = build_client_config(options.no_verification);

        let addr = "zmvpn.cczu.edu.cn:443";
        let connector = TlsConnector::from(config);
        let tcpstream = TcpStream::connect(addr)
            .await
            .with_context(|| format!("failed to connect to proxy server at {addr}"))?;
        check_start_cancelled(&cancel)?;
        let server_name = "zmvpn.cczu.edu.cn"
            .try_into()
            .map_err(|err| anyhow!("invalid proxy server name zmvpn.cczu.edu.cn: {err}"))?;
        let mut io = connector
            .connect(server_name, tcpstream)
            .await
            .context("failed to establish TLS with proxy server")?;
        check_start_cancelled(&cancel)?;
        let packet = AuthorizationPacket::new(authorization.data.token, user)
            .build()
            .context("failed to build authorization packet")?;
        io.write_all(packet.as_slice())
            .await
            .context("failed to send authorization packet")?;

        let mut proxy = consume_authization(&mut io).await?;
        check_start_cancelled(&cancel)?;
        proxy.dns = realdns;
        proxy.split_tunnel_routes = split_tunnel_routes;

        let (reader_half, writer_half) = split(io);
        let (command_tx, command_rx) = mpsc::channel(32);
        let (packet_queue_tx, packet_queue_rx) = mpsc::channel(PACKET_QUEUE_CAPACITY);
        let (packet_tx, _) = broadcast::channel(POLLER_BROADCAST_CAPACITY);

        let task = tokio::spawn(run_session_actor(
            session_id,
            PacketStreamReader::new(reader_half),
            writer_half,
            command_rx,
            packet_queue_tx,
            packet_tx.clone(),
        ));

        Ok(SessionHandle {
            id: session_id,
            server: proxy,
            command_tx,
            packet_rx: Arc::new(Mutex::new(packet_queue_rx)),
            packet_tx,
            task,
        })
    }
    .await;

    let mut guard = lock_session()?;
    match start_result {
        Ok(handle) => {
            info!(
                session_id = handle.id,
                address = %handle.server.address,
                mask = %handle.server.mask,
                dns = %handle.server.dns,
                split_tunnel_route_count = handle.server.split_tunnel_routes.len(),
                "proxy service started"
            );
            guard.replace(SessionSlot::Running(handle));
            Ok(())
        }
        Err(err) => {
            let should_clear = match guard.as_ref() {
                Some(SessionSlot::Starting(handle)) => handle.id == session_id,
                Some(SessionSlot::Running(handle)) => handle.id == session_id,
                None => false,
            };
            if should_clear {
                let _ = guard.take();
            }
            Err(err)
        }
    }
}

pub async fn start_service(user: impl Into<String>, password: impl Into<String>) -> Result<()> {
    start_service_with_options(user, password, StartOptions::default()).await
}

pub async fn service_available() -> bool {
    let Ok(guard) = SESSION.lock() else {
        return false;
    };
    matches!(
        guard.as_ref(),
        Some(SessionSlot::Running(handle)) if !handle.task.is_finished()
    )
}

pub async fn stop_service() -> Result<()> {
    info!("stopping proxy service");
    stop_polling_packet();
    waiting_polling_packet_stop().context("poller thread panicked while stopping")?;

    let session = {
        let mut guard = lock_session()?;
        guard.take()
    };

    match session {
        Some(SessionSlot::Running(handle)) => {
            let _ = handle.command_tx.send(SessionCommand::Stop).await;
            handle
                .task
                .await
                .context("session actor task join failed")??;
        }
        Some(SessionSlot::Starting(handle)) => {
            handle.cancel.store(true, Ordering::Relaxed);
        }
        None => {}
    }

    Ok(())
}

pub async fn receive_packet(_size: u32) -> Result<Vec<u8>> {
    let packet_rx: PacketQueueHandle = {
        let guard = lock_session()?;
        match guard.as_ref() {
            Some(SessionSlot::Running(handle)) => handle.packet_rx.clone(),
            Some(SessionSlot::Starting(_)) => bail!("service is still starting"),
            None => bail!("service is not running"),
        }
    };

    let mut rx_guard: tokio::sync::MutexGuard<'_, PacketQueueReceiver> = packet_rx.lock().await;
    rx_guard.recv().await.context("session packet queue closed")
}

pub async fn send_packet(packet: &[u8]) -> Result<()> {
    let (response_tx, response_rx) = oneshot::channel();
    request_command(
        SessionCommand::SendRaw {
            packet: packet.to_vec(),
            response: response_tx,
        },
        response_rx,
    )
    .await
}

pub async fn send_tcp_packet(packet: &[u8]) -> Result<()> {
    let (response_tx, response_rx) = oneshot::channel();
    request_command(
        SessionCommand::SendTcp {
            packet: packet.to_vec(),
            response: response_tx,
        },
        response_rx,
    )
    .await
}

pub async fn send_heartbeat() -> Result<()> {
    let (response_tx, response_rx) = oneshot::channel();
    request_command(
        SessionCommand::SendHeartbeat {
            response: response_tx,
        },
        response_rx,
    )
    .await
}

pub fn start_polling_packet(callback: impl Send + 'static + Fn(u32, Vec<u8>) -> ()) -> Result<()> {
    stop_polling_packet();
    waiting_polling_packet_stop().context("poller thread panicked while restarting")?;

    let packet_tx: PacketBroadcastSender = {
        let guard = lock_session()?;
        match guard.as_ref() {
            Some(SessionSlot::Running(handle)) => handle.packet_tx.clone(),
            Some(SessionSlot::Starting(_)) => bail!("service is still starting"),
            None => bail!("service is not running"),
        }
    };

    POLLER_SIGNAL.store(false, Ordering::Relaxed);
    let handler = thread::Builder::new()
        .name(String::from("vpn-poller"))
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Create polling runtime failed!");
            runtime.block_on(async move {
                let mut packet_rx: broadcast::Receiver<Vec<u8>> = packet_tx.subscribe();
                loop {
                    if POLLER_SIGNAL.load(Ordering::Relaxed) {
                        break;
                    }
                    match time::timeout(Duration::from_millis(500), packet_rx.recv()).await {
                        Ok(Ok(packet)) => callback(packet.len() as u32, packet),
                        Ok(Err(broadcast::error::RecvError::Lagged(skipped))) => {
                            warn!(
                                skipped_packets = skipped,
                                "poller lagged and skipped packets"
                            );
                        }
                        Ok(Err(broadcast::error::RecvError::Closed)) => break,
                        Err(_) => {}
                    }
                }
                POLLER_SIGNAL.store(false, Ordering::Relaxed);
            });
        })
        .context("failed to spawn poller thread")?;

    let mut guard = POLLER
        .lock()
        .map_err(|_| anyhow!("poller state lock poisoned"))?;
    guard.replace(handler);
    info!("started packet polling thread");

    Ok(())
}

pub fn stop_polling_packet() {
    POLLER_SIGNAL.store(true, Ordering::Relaxed);
    info!("requested packet polling stop");
}

pub fn waiting_polling_packet_stop() -> Result<()> {
    let mut guard = POLLER
        .lock()
        .map_err(|_| anyhow!("poller state lock poisoned"))?;
    if let Some(handler) = guard.take() {
        handler
            .join()
            .map_err(|_| anyhow!("poller thread panicked"))?;
    }
    Ok(())
}
