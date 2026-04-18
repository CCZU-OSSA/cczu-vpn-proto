use anyhow::{Context, Result, bail, ensure};
use tokio::io::{AsyncReadExt, ReadHalf};
use tokio::net::TcpStream;
use tokio::time::{Duration, timeout};
use tokio_rustls::client;

pub const INBOUND_FRAME_HEADER_LEN: usize = 8;

pub type ProxyStream = client::TlsStream<TcpStream>;
pub type ProxyReadHalf = ReadHalf<ProxyStream>;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InboundFrameKind {
    Control,
    Data,
    Unknown(u8),
}

impl InboundFrameKind {
    fn from_wire(value: u8) -> Self {
        match value {
            2 => Self::Control,
            4 => Self::Data,
            other => Self::Unknown(other),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct InboundFrameHeader {
    pub version: u8,
    pub kind: InboundFrameKind,
    pub total_len: u16,
    pub xid: [u8; 4],
}

impl InboundFrameHeader {
    fn parse(header: &[u8; INBOUND_FRAME_HEADER_LEN]) -> Result<Self> {
        ensure!(
            header[0] == 1,
            "invalid inbound frame version: {}",
            header[0]
        );

        let total_len = u16::from_be_bytes([header[2], header[3]]);
        ensure!(
            usize::from(total_len) >= INBOUND_FRAME_HEADER_LEN,
            "invalid inbound frame length: {total_len}"
        );

        Ok(Self {
            version: header[0],
            kind: InboundFrameKind::from_wire(header[1]),
            total_len,
            xid: [header[4], header[5], header[6], header[7]],
        })
    }

    fn is_control_frame(&self) -> bool {
        matches!(self.kind, InboundFrameKind::Control) && matches!(self.total_len, 10 | 12)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum InboundFrame {
    Control {
        header: InboundFrameHeader,
        payload: Vec<u8>,
    },
    Data {
        header: InboundFrameHeader,
        payload: Vec<u8>,
    },
}

#[derive(Debug)]
pub struct PacketStreamReader {
    buffer: Vec<u8>,
    stream: ProxyReadHalf,
}

impl PacketStreamReader {
    pub fn new(stream: ProxyReadHalf) -> Self {
        Self {
            buffer: Vec::new(),
            stream,
        }
    }

    async fn read_into_buffer(&mut self, timeout_duration: Duration) -> Result<bool> {
        let mut chunk = [0u8; 4096];
        match timeout(timeout_duration, self.stream.read(&mut chunk)).await {
            Ok(Ok(0)) => bail!("proxy connection closed"),
            Ok(Ok(got)) => {
                self.buffer.extend_from_slice(&chunk[..got]);
                Ok(true)
            }
            Ok(Err(err)) => Err(err).context("failed reading proxy stream"),
            Err(_) => Ok(false),
        }
    }

    async fn ensure_buffer_len(
        &mut self,
        required_len: usize,
        timeout_duration: Duration,
    ) -> Result<bool> {
        while self.buffer.len() < required_len {
            if !self.read_into_buffer(timeout_duration).await? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn take_frame(&mut self, len: usize) -> Vec<u8> {
        let remaining = self.buffer.split_off(len);
        std::mem::replace(&mut self.buffer, remaining)
    }

    pub async fn try_read_frame(&mut self) -> Result<Option<InboundFrame>> {
        if !self
            .ensure_buffer_len(INBOUND_FRAME_HEADER_LEN, Duration::from_millis(500))
            .await?
        {
            return Ok(None);
        }

        let raw_header: [u8; INBOUND_FRAME_HEADER_LEN] = self.buffer[..INBOUND_FRAME_HEADER_LEN]
            .try_into()
            .expect("slice length is checked");
        let header = InboundFrameHeader::parse(&raw_header)?;
        let total_len = usize::from(header.total_len);

        if !self
            .ensure_buffer_len(total_len, Duration::from_secs(5))
            .await?
        {
            return Ok(None);
        }

        let frame = self.take_frame(total_len);
        let payload = frame[INBOUND_FRAME_HEADER_LEN..].to_vec();

        if header.is_control_frame() {
            return Ok(Some(InboundFrame::Control { header, payload }));
        }

        Ok(Some(InboundFrame::Data { header, payload }))
    }
}
