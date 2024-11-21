use std::io::Write;

use byteorder::{BigEndian, WriteBytesExt};

pub trait Packet {
    fn build(&self) -> Vec<u8>;
}

pub struct AuthorizationPacket {
    token: String,
    user: String,
}
impl AuthorizationPacket {
    pub fn new(token: String, user: String) -> Self {
        Self { token, user }
    }
}

impl Packet for AuthorizationPacket {
    fn build(&self) -> Vec<u8> {
        let bytes_token = self.token.as_bytes();
        let bytes_user = self.user.as_bytes();
        let mut data = vec![];
        // Version
        data.write_u8(1).unwrap();
        // Protocal
        data.write_u8(1).unwrap();
        // Length
        data.write_u16::<BigEndian>(19 + bytes_user.len() as u16 + bytes_token.len() as u16)
            .unwrap();
        data.write_all(&[
            0, 0, 0, 0, // Zero
            1, 0, 0, 0, // ELK_METHOD_STUN
            1, 0, // ELK_OPT_USERNAME
        ])
        .unwrap();
        // User
        data.write_u8(bytes_user.len() as u8).unwrap();
        data.write(bytes_user).unwrap();
        // ELK_OPT_SESSID
        data.write(&[2, 0]).unwrap();

        // Token
        data.write_u8(bytes_token.len() as u8).unwrap();
        data.write(bytes_token).unwrap();

        data.write_i8(-1).unwrap();

        data.flush().unwrap();

        data
    }
}
