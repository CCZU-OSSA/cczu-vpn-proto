<div align=center>
  <img width=200 src="doc\logo.png"  alt="logo"/>
  <h1 align="center">CCZU-VPN-PROTO</h1>
</div>

<div align=center>
    <img src="https://img.shields.io/badge/Rust-2024-brown" alt="Rust">
    <img src="https://img.shields.io/github/languages/code-size/CCZU-OSSA/cczu-vpn-proto?color=green" alt="size">
</div>

## Features

```rust
todo!()
```

## Usage

### For Rust

```sh
cargo add --git https://github.com/CCZU-OSSA/cczu-vpn-proto.git
```

```rust
service::start_service("user".trim(), "password".trim()).await;
service::start_polling_packet(move |a, b| {
    println!("rev datasize: {a}");
    // TODO: proxy to tun/tap
});
loop{
    // TODO: read raw packet from tun/tap
    if !service::send_tcp_packet(&mut buf[..len]).await {
        println!("packet send failed");
    }
    if service::POLLER_SIGNAL.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(());
    }
}
```

### For C++

Install rust toolchain for your platform.

```sh
rustup target add xxxxx
cargo build --release --target=xxxxx
```

Find library in `target/release/`

Load library and use.
