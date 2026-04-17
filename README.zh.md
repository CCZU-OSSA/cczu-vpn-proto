<div align=center>
  <img width=200 src="doc\logo.png" alt="logo"/>
  <h1 align="center">CCZU-VPN-PROTO</h1>
</div>

<div align=center>
  <img src="https://img.shields.io/badge/Rust-2024-brown" alt="Rust">
  <img src="https://img.shields.io/github/languages/code-size/CCZU-OSSA/cczu-vpn-proto?color=green" alt="size">
</div>

CCZU WebVPN 隧道客户端的 Rust 开源重实现。

这个项目当前同时提供两条使用路径：

- 一条面向直接使用的 Windows CLI 客户端路径
- 一条面向嵌入式接入的 Rust 库 / UniFFI bindings 路径

## 特性

- 基于 `tun-rs` 的跨平台 TUN 设备接入
- 面向直接使用的 Windows Wintun CLI 路径
- 根据 WebVPN 规则数据自动安装 split-tunnel 路由
- 通过 UniFFI 导出 Kotlin、Swift、Python、Ruby 绑定
- 基于 `tracing` 的 CLI 与库级诊断日志

## CLI Usage Guide

CLI 入口位于 [src/main.rs](./src/main.rs)，当前已经可以直接使用，尤其是 Windows 路径。
目前最完整的路径是 Windows，因为它会自动配置 Wintun、DNS 和 split-tunnel 路由。

### 面向对象

- 想直接使用 Windows CLI 客户端的用户
- 需要对照真实服务验证协议行为的开发者
- 想在本机快速打通 native tunnel 链路的集成方

### 环境要求

- 已安装 Rust toolchain
- Windows 下需要管理员权限
- 能访问 `zmvpn.cczu.edu.cn`

### 运行

PowerShell：

```powershell
$env:RUST_LOG = "info"
cargo run --release
```

### CLI 会做什么

1. 检查 WebVPN 当前是否看起来可用。
2. 提示输入用户名和密码。
3. 启动隧道会话。
4. 创建一个名为 `CCZU-VPN-PROTO` 的 `tun-rs` 虚拟网卡。
5. 在 Windows 上：
   - 需要时在当前工作目录落地 `wintun.dll`
   - 为虚拟网卡设置 VPN DNS
   - 根据 WebVPN 返回的校园网段安装 split-tunnel 路由
6. 在 TUN 设备与自定义 VPN 协议之间做双向包转发。

### 说明

- CLI 默认使用库里的 TLS 选项，也就是当前的 `no_verification = true`。
- 正常退出时会尝试删除 split-tunnel 路由。
- 包收发和路由安装日志都受 `RUST_LOG` 控制。
- 如果你是把它嵌入到别的应用里，通常更适合直接使用 Rust API 或 UniFFI bindings，而不是外部调用 CLI。

## Library Usage

### Rust API

先添加依赖：

```sh
cargo add --git https://github.com/CCZU-OSSA/cczu-vpn-proto.git
```

最小异步示例：

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

导出的绑定入口在 [src/bindings.rs](./src/bindings.rs)。
当前支持生成的绑定语言：

- Kotlin
- Swift
- Python
- Ruby

发布产物包含：

- 原生库文件（按目标平台提供 `.dll`、`.so`、`.dylib`、`.a`、`.lib`）
- 每种支持语言对应的 UniFFI 绑定压缩包

## Develop Guide

### 项目结构

- [src/main.rs](./src/main.rs)：CLI 测试入口
- [src/bindings.rs](./src/bindings.rs)：UniFFI 导出层
- [src/types.rs](./src/types.rs)：共享类型和启动选项
- [src/vpn/service.rs](./src/vpn/service.rs)：会话 actor 和运行时状态
- [src/vpn/protocol](./src/vpn/protocol)：自定义协议的读写实现
- [src/diag.rs](./src/diag.rs)：`tracing` 初始化辅助

### 常用命令

格式化：

```sh
cargo fmt
```

检查：

```sh
cargo check --locked
```

构建 release：

```sh
cargo build --release --locked
```

只编译测试目标，不执行：

```sh
cargo test --locked --no-run
```

### 本地生成 UniFFI bindings

先启用 bindgen feature：

```sh
cargo build --release --locked --features bindgen
cargo build --locked --features bindgen --bin uniffi-bindgen
```

然后基于宿主机生成的动态库，按语言分别生成 bindings：

Windows 宿主：

```powershell
.\target\debug\uniffi-bindgen.exe generate -n -l kotlin -o .\dist\bindings\kotlin .\target\release\cczuvpnproto.dll
.\target\debug\uniffi-bindgen.exe generate -n -l swift -o .\dist\bindings\swift .\target\release\cczuvpnproto.dll
.\target\debug\uniffi-bindgen.exe generate -n -l python -o .\dist\bindings\python .\target\release\cczuvpnproto.dll
.\target\debug\uniffi-bindgen.exe generate -n -l ruby -o .\dist\bindings\ruby .\target\release\cczuvpnproto.dll
```

Linux 宿主：

```sh
target/debug/uniffi-bindgen generate -n -l kotlin -o ./dist/bindings/kotlin ./target/release/libcczuvpnproto.so
```

macOS 宿主：

```sh
target/debug/uniffi-bindgen generate -n -l swift -o ./dist/bindings/swift ./target/release/libcczuvpnproto.dylib
```

### 发布工作流

- [nightly.yml](./.github/workflows/nightly.yml)：定时或手动触发的 `pre-release` 预发布工作流
- [release.yml](./.github/workflows/release.yml)：推送 `v*` tag 时触发的正式发布工作流

两条 workflow 都会发布：

- matrix 中各目标平台的原生库
- Kotlin、Swift、Python、Ruby 的 UniFFI 绑定压缩包
