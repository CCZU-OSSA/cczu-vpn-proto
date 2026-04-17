use std::io::Write;

use anyhow::{Context, Result};
use byteorder::{BigEndian, WriteBytesExt};

pub trait Packet {
    fn build(&self) -> Result<Vec<u8>>;
}

pub struct AuthorizationPacket {
    token: String,
    user: String,
}

pub struct TCPPacket<'a> {
    data: &'a [u8],
}

impl<'a> TCPPacket<'a> {
    pub fn new(data: &'a [u8]) -> TCPPacket<'a> {
        Self { data }
    }
}

impl<'a> Packet for TCPPacket<'a> {
    fn build(&self) -> Result<Vec<u8>> {
        // Custom Header
        let mut packet = vec![1, 4];
        let total_len = self
            .data
            .len()
            .checked_add(12)
            .context("TCP packet length overflow")?;
        let total_len = u16::try_from(total_len)
            .with_context(|| format!("TCP packet too large: {} bytes", self.data.len()))?;

        // Length
        packet
            .write_u16::<BigEndian>(total_len)
            .context("failed to encode TCP packet length")?;
        // XID
        packet
            .write_all(&[0, 0, 0, 0])
            .context("failed to encode TCP packet xid")?;
        // APP ID
        packet
            .write_i32::<BigEndian>(1)
            .context("failed to encode TCP packet app id")?;
        // Data
        packet
            .write_all(self.data)
            .context("failed to encode TCP packet payload")?;

        Ok(packet)
    }
}

pub static HEARTBEAT: [u8; 12] = [1, 1, 0, 12, 0, 0, 0, 0, 3, 0, 0, 0];

impl AuthorizationPacket {
    pub fn new(token: String, user: String) -> Self {
        Self { token, user }
    }
}

impl Packet for AuthorizationPacket {
    fn build(&self) -> Result<Vec<u8>> {
        let bytes_token = self.token.as_bytes();
        let bytes_user = self.user.as_bytes();
        let mut data = vec![];
        let total_len = bytes_user
            .len()
            .checked_add(bytes_token.len())
            .and_then(|len| len.checked_add(19))
            .context("authorization packet length overflow")?;
        let total_len =
            u16::try_from(total_len).context("authorization packet too large for protocol")?;
        // Version
        data.write_u8(1)
            .context("failed to encode authorization packet version")?;
        // Protocal
        data.write_u8(1)
            .context("failed to encode authorization packet protocol")?;
        // Length
        data.write_u16::<BigEndian>(total_len)
            .context("failed to encode authorization packet length")?;
        data.write_all(&[
            0, 0, 0, 0, // Zero
            1, 0, 0, 0, // ELK_METHOD_STUN
            1, 0, // ELK_OPT_USERNAME
        ])
        .context("failed to encode authorization packet header")?;
        // User
        data.write_u8(u8::try_from(bytes_user.len()).context("username too large for protocol")?)
            .context("failed to encode username length")?;
        data.write_all(bytes_user)
            .context("failed to encode username bytes")?;
        // ELK_OPT_SESSID
        data.write_all(&[2, 0])
            .context("failed to encode session field tag")?;

        // Token
        data.write_u8(u8::try_from(bytes_token.len()).context("token too large for protocol")?)
            .context("failed to encode token length")?;
        data.write_all(bytes_token)
            .context("failed to encode token bytes")?;

        data.write_i8(-1)
            .context("failed to encode authorization terminator")?;

        data.flush()
            .context("failed to flush authorization packet buffer")?;

        Ok(data)
    }
}
