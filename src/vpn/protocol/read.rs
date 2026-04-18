use anyhow::{Context, Result, bail, ensure};
use tokio::io::AsyncReadExt;

use crate::types::ProxyServer;

use super::stream::ProxyStream;

pub async fn consume_authization(stream: &mut ProxyStream) -> Result<ProxyServer> {
    stream
        .read_exact(&mut [0u8; 10])
        .await
        .context("failed to read authorization prefix")?;

    let mut auth_status = vec![0, 0];
    stream
        .read_exact(&mut auth_status)
        .await
        .context("failed to read authorization status")?;
    ensure!(
        auth_status == [0, 0],
        "authorization failed with status: {:?}",
        auth_status
    );

    let mut status = [0u8; 3];
    stream
        .read_exact(&mut status)
        .await
        .context("failed to read virtual address tag")?;
    ensure!(
        status == [11, 0, 4],
        "invalid virtual address tag: {:?}",
        status
    );

    let virtual_address = [
        stream
            .read_u8()
            .await
            .context("failed to read address octet 0")?,
        stream
            .read_u8()
            .await
            .context("failed to read address octet 1")?,
        stream
            .read_u8()
            .await
            .context("failed to read address octet 2")?,
        stream
            .read_u8()
            .await
            .context("failed to read address octet 3")?,
    ];

    stream
        .read_exact(&mut status)
        .await
        .context("failed to read mask tag")?;
    ensure!(
        status == [12, 0, 4],
        "invalid virtual mask tag: {:?}",
        status
    );

    let mut mask = [0u8; 4];
    stream
        .read_exact(&mut mask)
        .await
        .context("failed to read virtual mask")?;

    let mut gateway = [0u8; 4];
    let mut dns = String::default();
    let mut wins = String::default();

    loop {
        let mut field_tag = [0u8; 2];
        stream
            .read_exact(&mut field_tag)
            .await
            .context("failed to read auth response field tag")?;
        if field_tag[0] == 43 {
            break;
        }

        match field_tag {
            [35, 0] => {
                let length = stream
                    .read_u8()
                    .await
                    .context("failed to read gateway length")?;
                ensure!(length == 4, "invalid gateway length: {length}");
                stream
                    .read_exact(&mut gateway)
                    .await
                    .context("failed to read gateway bytes")?;
            }
            [36, 0] => {
                let length = stream
                    .read_u8()
                    .await
                    .context("failed to read DNS length")?;
                let mut data = vec![0u8; length as usize];
                stream
                    .read_exact(&mut data)
                    .await
                    .context("failed to read DNS bytes")?;
                dns = String::from_utf8(data).context("invalid DNS utf-8")?;
            }
            [37, 0] => {
                let length = stream
                    .read_u8()
                    .await
                    .context("failed to read WINS length")?;
                let mut data = vec![0u8; length as usize];
                stream
                    .read_exact(&mut data)
                    .await
                    .context("failed to read WINS bytes")?;
                wins = String::from_utf8(data).context("invalid WINS utf-8")?;
            }
            _ => bail!("invalid auth response field tag: {:?}", field_tag),
        }
    }

    loop {
        let bin = stream
            .read_u8()
            .await
            .context("failed to read auth response trailer")?;
        if bin == 255 {
            break;
        }
    }

    Ok(ProxyServer {
        address: virtual_address.map(|e| e.to_string()).join("."),
        mask: mask.map(|e| e.to_string()).join("."),
        gateway: gateway.map(|e| e.to_string()).join("."),
        dns,
        wins,
        split_tunnel_routes: Vec::new(),
    })
}
