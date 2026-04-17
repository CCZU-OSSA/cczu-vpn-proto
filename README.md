<div align=center>
  <img width=200 src="doc\logo.png" alt="logo"/>
  <h1 align="center">CCZU-VPN-PROTO</h1>
</div>

<div align=center>
  <img src="https://img.shields.io/badge/Rust-2024-brown" alt="Rust">
  <img src="https://img.shields.io/github/languages/code-size/CCZU-OSSA/cczu-vpn-proto?color=green" alt="size">
</div>

Open-source Rust reimplementation of the CCZU WebVPN tunnel client.

The project currently provides both:

- a Windows-oriented CLI client path for direct usage
- a Rust library and UniFFI bindings for embedding and integration

## Features

- `tun-rs` based cross-platform TUN device integration
- Windows Wintun CLI client path
- Split-tunnel route installation from WebVPN rule data
- UniFFI exports for Kotlin, Swift, Python, and Ruby
- `tracing` based diagnostics for both CLI and library consumers

## CLI Usage Guide

The CLI entrypoint in [src/main.rs](./src/main.rs) can be used directly, especially on Windows.
At the moment, Windows is the most complete path because it configures Wintun, DNS, and split-tunnel routes automatically.

### Intended audience

- Windows users who want to run the tunnel client directly
- Developers validating protocol behavior against the real service
- Integrators who want to smoke-test the native tunnel path locally

### Requirements

- Rust toolchain installed
- Administrator privileges on Windows
- Network access to `zmvpn.cczu.edu.cn`

### Run

PowerShell:

```powershell
$env:RUST_LOG = "info"
cargo run --release
```

### What the CLI does

1. Checks whether WebVPN appears to be available.
2. Prompts for username and password.
3. Starts the tunnel session.
4. Creates a `tun-rs` device named `CCZU-VPN-PROTO`.
5. On Windows:
   - materializes `wintun.dll` in the working directory when needed
   - sets the VPN DNS server on the virtual interface
   - installs split-tunnel routes for the campus network ranges returned by WebVPN
6. Bridges packets between the TUN device and the custom VPN protocol.

### Notes

- The CLI uses the library default TLS option, which currently means `no_verification = true`.
- Split-tunnel routes are removed on clean shutdown.
- Packet and routing logs respect `RUST_LOG`.
- If you are embedding this project into another application, the library API and UniFFI bindings are usually a better fit than shelling out to the CLI.

## Library Usage

### Rust API

Add the crate:

```sh
cargo add --git https://github.com/CCZU-OSSA/cczu-vpn-proto.git
```

Minimal async example:

```rust
use anyhow::Result;
use cczuvpnproto::{
    diag,
    types::StartOptions,
    vpn::service,
};

#[tokio::main]
async fn main() -> Result<()> {
    diag::init_tracing("info")?;

    service::start_service_with_options(
        "user",
        "password",
        StartOptions {
            no_verification: true,
        },
    )
    .await?;

    let server = service::proxy_server().expect("proxy server should exist after login");
    println!("{server:?}");
    Ok(())
}
```

### UniFFI bindings

The exported API lives in [src/bindings.rs](./src/bindings.rs).
Supported generated bindings:

- Kotlin
- Swift
- Python
- Ruby

Published releases include:

- native libraries (`.dll`, `.so`, `.dylib`, `.a`, `.lib` when available)
- UniFFI binding archives for each supported language

## Develop Guide

### Project layout

- [src/main.rs](./src/main.rs): CLI test entrypoint
- [src/bindings.rs](./src/bindings.rs): UniFFI exported surface
- [src/types.rs](./src/types.rs): shared records and options
- [src/vpn/service.rs](./src/vpn/service.rs): session actor and runtime state
- [src/vpn/protocol](./src/vpn/protocol): custom protocol reader and writer
- [src/diag.rs](./src/diag.rs): tracing initialization helpers

### Common commands

Format:

```sh
cargo fmt
```

Check:

```sh
cargo check --locked
```

Build release artifacts:

```sh
cargo build --release --locked
```

Compile test targets without running them:

```sh
cargo test --locked --no-run
```

### Generate UniFFI bindings locally

Enable the bindgen feature first:

```sh
cargo build --release --locked --features bindgen
cargo build --locked --features bindgen --bin uniffi-bindgen
```

Then generate bindings one language at a time from the host library:

Windows host:

```powershell
.\target\debug\uniffi-bindgen.exe generate -n -l kotlin -o .\dist\bindings\kotlin .\target\release\cczuvpnproto.dll
.\target\debug\uniffi-bindgen.exe generate -n -l swift -o .\dist\bindings\swift .\target\release\cczuvpnproto.dll
.\target\debug\uniffi-bindgen.exe generate -n -l python -o .\dist\bindings\python .\target\release\cczuvpnproto.dll
.\target\debug\uniffi-bindgen.exe generate -n -l ruby -o .\dist\bindings\ruby .\target\release\cczuvpnproto.dll
```

Linux host:

```sh
target/debug/uniffi-bindgen generate -n -l kotlin -o ./dist/bindings/kotlin ./target/release/libcczuvpnproto.so
```

macOS host:

```sh
target/debug/uniffi-bindgen generate -n -l swift -o ./dist/bindings/swift ./target/release/libcczuvpnproto.dylib
```

### Release workflows

- [nightly.yml](./.github/workflows/nightly.yml): scheduled or manual pre-release build for the `pre-release` tag
- [release.yml](./.github/workflows/release.yml): stable release workflow triggered by pushing a `v*` tag

Both workflows publish:

- native libraries for each target in the matrix
- UniFFI binding archives for Kotlin, Swift, Python, and Ruby
