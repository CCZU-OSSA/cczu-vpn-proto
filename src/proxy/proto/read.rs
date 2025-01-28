use std::io::ErrorKind;

use tokio::io::AsyncReadExt;

use crate::{model::ProxyServer, proxy::service::PROXY};

// Read after auth
pub async fn consume_authization() -> Result<ProxyServer, tokio::io::Error> {
    let mut guard = PROXY.lock().await;

    let stream = guard.as_mut().ok_or(tokio::io::Error::new(
        ErrorKind::NotConnected,
        "Please connect to proxy first.",
    ))?;
    // 1. Auth Status
    // Skip 10
    stream.read_exact(&mut [0u8; 10]).await?;

    let mut auth_status = vec![0, 0];
    stream.read_exact(&mut auth_status).await?;
    if !(auth_status[0] == 0 && auth_status[1] == 0) {
        return Err(tokio::io::Error::new(ErrorKind::Other, "Authorize Failed."));
    }

    // 2. virtual_address
    let mut status = [0u8; 3];
    stream.read_exact(&mut status).await?;

    let virtual_address: [u8; 4];
    if status[0] == 11 && status[1] == 0 && status[2] == 4 {
        virtual_address = [
            stream.read_u8().await? & 255,
            stream.read_u8().await? & 255,
            stream.read_u8().await? & 255,
            stream.read_u8().await? & 255,
        ];
    } else {
        return Err(tokio::io::Error::new(
            ErrorKind::InvalidData,
            format!("Invalid Data Header `{:?}`", status),
        ));
    }

    // 3. virtual_mask
    let mut raw_mask = 0;
    let mut status = [0u8; 3];
    stream.read_exact(&mut status).await?;
    if status[0] == 12 && status[1] == 0 && status[2] == 4 {
        let mut a = 0;
        let mut b = 0;
        let mut data = vec![0u8; 4];
        stream.read_exact(&mut data).await?;
        data.iter().for_each(|val| {
            let val = val & 255;
            let binary = format!("{val:b}");
            while let Some(pos) = binary
                .chars()
                .enumerate()
                .position(|(pos, char)| pos >= b && char == '1')
            {
                a = pos + 1;
                b += 1;
            }

            raw_mask += b;
        });
    } else {
        return Err(tokio::io::Error::new(
            ErrorKind::InvalidData,
            format!("Invalid Data Header `{:?}`", status),
        ));
    }

    let mut vec_mask = vec![true; raw_mask];
    vec_mask.append(&mut vec![false; 32 - raw_mask]);
    let chucks_mask: Vec<u8> = vec_mask
        .chunks(8)
        .map(|chuck| {
            u8::from_str_radix(
                &chuck.iter().fold(String::new(), |val, e| {
                    if *e {
                        format!("{val}1")
                    } else {
                        format!("{val}0")
                    }
                }),
                2,
            )
            .unwrap()
        })
        .collect();

    let mask: [u8; 4] = chucks_mask[0..4].try_into().unwrap();

    // 4. gateway/dns/wins
    let mut gateway = [0u8; 4];
    let mut dns = String::default();
    let mut wins = String::default();

    loop {
        let mut status = [0u8; 2];
        stream.read_exact(&mut status).await?;
        if status[0] != 43 {
            match status {
                [35, 0] => {
                    let length = stream.read_u8().await?;
                    let mut data = vec![0u8; length as usize];
                    stream.read_exact(&mut data).await?;

                    gateway = [data[0] & 255, data[1] & 255, data[2] & 255, data[3] & 255];
                }
                [36, 0] => {
                    let length = stream.read_u8().await?;
                    let mut data = vec![0u8; length as usize];
                    stream.read_exact(&mut data).await?;

                    dns = String::from_utf8(data).map_err(|e| {
                        tokio::io::Error::new(ErrorKind::InvalidData, e.to_string())
                    })?;
                }
                [37, 0] => {
                    let length = stream.read_u8().await?;
                    let mut data = vec![0u8; length as usize];
                    stream.read_exact(&mut data).await?;

                    wins = String::from_utf8(data).map_err(|e| {
                        tokio::io::Error::new(ErrorKind::InvalidData, e.to_string())
                    })?;
                }
                _ => {
                    return Err(tokio::io::Error::new(
                        ErrorKind::InvalidData,
                        format!("Invalid Status {:?}", status),
                    ))
                }
            };
        } else {
            break;
        }
    }

    // 5. empty_data
    loop {
        let bin = stream.read_u8().await?;
        if bin == 255 {
            break;
        }
    }

    // Return data here
    Ok(ProxyServer {
        address: virtual_address.map(|e| e.to_string()).join("."),
        mask: mask.map(|e| e.to_string()).join("."),
        gateway: gateway.map(|e| e.to_string()).join("."),
        dns,
        wins,
    })
}

pub async fn try_read_packet_data() -> Result<Option<Vec<u8>>, tokio::io::Error> {
    let mut guard = PROXY.lock().await;

    let stream = guard.as_mut().ok_or(tokio::io::Error::new(
        ErrorKind::NotConnected,
        "Please connect to proxy first.",
    ))?;

    let mut header = [0u8; 8];

    let got = stream.read_exact(&mut header).await?;
    if got <= 0 {
        return Ok(None);
    }

    if header[0] != 1 || header[1] != 2 || header[2] != 0 || header[3] != 10 {
        let len = u16::from_le_bytes([header[3], header[2]]) - 8;
        let mut data = vec![0u8; len.into()];
        stream.read_exact(&mut data).await?;
        return Ok(Some(data));
    } else {
        stream.read(&mut [0u8; 2048]).await?;
    }
    return Ok(None);
}

#[test]
fn conv() {
    println!("{}", u16::from_le_bytes([90u8, 200u8]));
    println!("{}", (90u16 & 255) | (200u16 << 8));
}
