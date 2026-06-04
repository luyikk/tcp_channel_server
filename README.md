# tcp-channel-server

[![Crates.io](https://img.shields.io/crates/v/tcp-channel-server.svg)](https://crates.io/crates/tcp-channel-server)
[![Documentation](https://docs.rs/tcp_channel_server/badge.svg)](https://docs.rs/tcp_channel_server)
[![License](https://img.shields.io/crates/l/tcp-channel-server.svg)](LICENSE)

**[English](#english) | [中文](#chinese)**

---

<a name="english"></a>

# tcp-channel-server — English

> **A lightweight, async TCP server framework for Rust** built on top of [Tokio](https://tokio.rs/).

- Minimum supported Rust version: **1.75+**
- Fluent builder API for easy configuration
- Per-connection stream initialization hook (plain TCP, TLS, or any custom wrapper)
- Optional connection filter callback to accept or reject incoming peers
- Channel-based peer abstraction for safe, concurrent sends
- TLS support via `openssl` (opt-in feature flag)

---

## Features

| Feature | Description |
|---|---|
| 🔌 Async TCP Server | Powered by Tokio, fully asynchronous accept and I/O |
| 🏗️ Fluent Builder | Chainable configuration methods with compile-time safety |
| 🔐 TLS Support | Opt-in via `tls` feature flag, uses `openssl` + `tokio-openssl` |
| 📡 Channel-Based Writes | Per-connection writer task eliminates lock contention on the write side |
| 🎛️ Stream Init Hook | Transform raw `TcpStream` into any stream type (`SslStream`, etc.) |
| 🔗 Connect Filter | Accept or reject connections by `SocketAddr` before spawning a handler |
| 🏷️ User Token | Generic `T` cloned into every connection handler — share state without globals |

## Requirements

- Rust **1.75** or later (edition 2021)

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
tcp-channel-server = "0.3"
tokio = { version = "1", features = ["full"] }
```

To enable TLS support:

```toml
[dependencies]
tcp-channel-server = { version = "0.3", features = ["tls"] }
```

## Quick Start

### Echo Server

```rust
use anyhow::Result;
use std::sync::Arc;
use tcp_channel_server::{Builder, ITCPServer};
use tokio::io::AsyncReadExt;

#[tokio::main]
async fn main() -> Result<()> {
    let server: Arc<dyn ITCPServer<()>> = Builder::new("0.0.0.0:5555")
        // Optional: accept or reject incoming connections by address
        .set_connect_event(|addr| {
            println!("{addr:?} connected");
            true
        })
        // Required: initialize the stream (plain TCP here; swap in TLS or any wrapper)
        .set_stream_init(|tcp_stream| async move { Ok(tcp_stream) })
        // Required: handle incoming data for each connection
        .set_input_event(|mut reader, peer, _token| async move {
            let mut buf = [0u8; 4096];
            loop {
                let n = reader.read(&mut buf).await?;
                if n == 0 {
                    break; // connection closed
                }
                peer.send(buf[..n].to_vec()).await?;
            }
            println!("{:?} disconnected", peer.addr());
            Ok(())
        })
        .build()
        .await;

    // Block the current task until the server stops
    server.start_block(()).await?;
    Ok(())
}
```

### TLS Server

```rust
use anyhow::Result;
use std::pin::Pin;
use std::sync::Arc;
use lazy_static::lazy_static;
use openssl::ssl::{Ssl, SslAcceptor, SslFiletype, SslMethod, SslVerifyMode};
use tcp_channel_server::{Builder, ITCPServer};
use tokio::io::AsyncReadExt;
use tokio_openssl::SslStream;

lazy_static! {
    static ref SSL: SslAcceptor = {
        let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        acceptor.set_ca_file("tests/chain.cert.pem").unwrap();
        acceptor.set_private_key_file("tests/server-key.pem", SslFiletype::PEM).unwrap();
        acceptor.set_certificate_chain_file("tests/server-cert.pem").unwrap();
        acceptor.check_private_key().unwrap();
        acceptor.build()
    };
}

#[tokio::main]
async fn main() -> Result<()> {
    let server: Arc<dyn ITCPServer<()>> = Builder::new("0.0.0.0:5555")
        .set_connect_event(|addr| {
            println!("{addr:?} connected");
            true
        })
        // Wrap the raw TcpStream in an SslStream
        .set_stream_init(|tcp_stream| async move {
            let ssl = Ssl::new(SSL.context())?;
            let mut stream = SslStream::new(ssl, tcp_stream)?;
            Pin::new(&mut stream).accept().await?;
            Ok(stream)
        })
        .set_input_event(|mut reader, peer, _| async move {
            let mut buf = [0u8; 4096];
            loop {
                let n = reader.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                peer.send(b"200\r\n".to_vec()).await?;
            }
            Ok(())
        })
        .build()
        .await;

    server.start_block(()).await?;
    Ok(())
}
```

## API Overview

### `Builder`

The entry point for constructing a server. All methods consume `self` and return `Self`, enabling a fluent chain.

| Method | Required | Description |
|---|---|---|
| `Builder::new(addr)` | ✓ | Bind address (anything that implements `ToSocketAddrs`) |
| `.set_stream_init(fn)` | ✓ | Transform a raw `TcpStream` into the final stream type `C` |
| `.set_input_event(fn)` | ✓ | Async handler called once per connection with `(ReadHalf<C>, Arc<TCPPeer<C>>, T)` |
| `.set_connect_event(fn)` | — | Return `true` to accept, `false` to reject an incoming address |
| `.build().await` | ✓ | Consumes the builder and returns `Arc<dyn ITCPServer<T>>` |

### `ITCPServer<T>`

```rust
pub trait ITCPServer<T> {
    /// Start accepting connections; returns a JoinHandle.
    async fn start(&self, token: T) -> anyhow::Result<JoinHandle<anyhow::Result<()>>>;

    /// Start accepting connections and block until the server stops.
    async fn start_block(&self, token: T) -> anyhow::Result<()>;
}
```

`T` is an arbitrary user token that is cloned and passed into every `input_event` invocation —
useful for sharing state (e.g. `Arc<AppState>`) across connections.

### `TCPPeer<C>`

Represents a connected client. It is cheaply cloneable via `Arc` and safe to use from multiple tasks.

| Method | Description |
|---|---|
| `peer.addr()` | Remote `SocketAddr` |
| `peer.send(buf)` | Enqueue bytes for writing (buffered, no flush) |
| `peer.send_all(buf)` | Enqueue bytes and flush |
| `peer.flush()` | Flush the write buffer |
| `peer.disconnect()` | Gracefully shut down the connection |
| `peer.is_disconnect()` | Check whether the peer is already disconnected |

## Architecture

See [CLAUDE.md](CLAUDE.md) for a detailed walkthrough of the internal architecture, including the connection lifecycle, generic parameters, and dependency roles.

## Related Crates

- **[tcpclient](https://crates.io/crates/tcpclient)** — Async TCP client companion crate

## License

Licensed under either of

- Apache License, Version 2.0
- MIT License

at your option.

---

<a name="chinese"></a>

# tcp-channel-server — 中文

> **轻量级异步 TCP 服务端框架**，基于 [Tokio](https://tokio.rs/) 构建。

- 最低支持 Rust 版本：**1.75+**
- 流式 Builder API，链式配置，简洁易用
- 每个连接可自定义流初始化（原生 TCP、TLS 或任意包装类型）
- 可选的连接过滤器，按地址接受或拒绝接入的 peer
- 基于 Channel 的 peer 抽象，安全并发发送
- 通过 `openssl` 支持 TLS（可选 feature 标志）

---

## 功能特性

| 功能 | 描述 |
|---|---|
| 🔌 异步 TCP 服务端 | 基于 Tokio，全异步 accept 和 I/O |
| 🏗️ 流式 Builder | 链式配置方法，编译期类型安全 |
| 🔐 TLS 支持 | 通过 `tls` feature 标志启用，基于 `openssl` + `tokio-openssl` |
| 📡 Channel 写入 | 每个连接独立的 writer 任务，消除写端锁竞争 |
| 🎛️ Stream Init 钩子 | 将原始 `TcpStream` 转换为任意流类型（如 `SslStream`） |
| 🔗 连接过滤 | 在生成 handler 之前按 `SocketAddr` 接受或拒绝连接 |
| 🏷️ 用户 Token | 泛型 `T` 被克隆到每个连接处理函数中 — 无需全局变量即可共享状态 |

## 环境要求

- Rust **1.75** 或更高版本（edition 2021）

## 安装

在 `Cargo.toml` 中添加：

```toml
[dependencies]
tcp-channel-server = "0.3"
tokio = { version = "1", features = ["full"] }
```

启用 TLS 支持：

```toml
[dependencies]
tcp-channel-server = { version = "0.3", features = ["tls"] }
```

## 快速开始

### Echo 服务端

```rust
use anyhow::Result;
use std::sync::Arc;
use tcp_channel_server::{Builder, ITCPServer};
use tokio::io::AsyncReadExt;

#[tokio::main]
async fn main() -> Result<()> {
    let server: Arc<dyn ITCPServer<()>> = Builder::new("0.0.0.0:5555")
        // 可选：按地址接受或拒绝接入的连接
        .set_connect_event(|addr| {
            println!("{addr:?} connected");
            true
        })
        // 必选：初始化流（此处为原生 TCP；可替换为 TLS 或任意包装类型）
        .set_stream_init(|tcp_stream| async move { Ok(tcp_stream) })
        // 必选：处理每个连接的传入数据
        .set_input_event(|mut reader, peer, _token| async move {
            let mut buf = [0u8; 4096];
            loop {
                let n = reader.read(&mut buf).await?;
                if n == 0 {
                    break; // 连接关闭
                }
                peer.send(buf[..n].to_vec()).await?;
            }
            println!("{:?} disconnected", peer.addr());
            Ok(())
        })
        .build()
        .await;

    // 阻塞当前任务直到服务端停止
    server.start_block(()).await?;
    Ok(())
}
```

### TLS 服务端

```rust
use anyhow::Result;
use std::pin::Pin;
use std::sync::Arc;
use lazy_static::lazy_static;
use openssl::ssl::{Ssl, SslAcceptor, SslFiletype, SslMethod, SslVerifyMode};
use tcp_channel_server::{Builder, ITCPServer};
use tokio::io::AsyncReadExt;
use tokio_openssl::SslStream;

lazy_static! {
    static ref SSL: SslAcceptor = {
        let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        acceptor.set_ca_file("tests/chain.cert.pem").unwrap();
        acceptor.set_private_key_file("tests/server-key.pem", SslFiletype::PEM).unwrap();
        acceptor.set_certificate_chain_file("tests/server-cert.pem").unwrap();
        acceptor.check_private_key().unwrap();
        acceptor.build()
    };
}

#[tokio::main]
async fn main() -> Result<()> {
    let server: Arc<dyn ITCPServer<()>> = Builder::new("0.0.0.0:5555")
        .set_connect_event(|addr| {
            println!("{addr:?} connected");
            true
        })
        // 将原始 TcpStream 包装为 SslStream
        .set_stream_init(|tcp_stream| async move {
            let ssl = Ssl::new(SSL.context())?;
            let mut stream = SslStream::new(ssl, tcp_stream)?;
            Pin::new(&mut stream).accept().await?;
            Ok(stream)
        })
        .set_input_event(|mut reader, peer, _| async move {
            let mut buf = [0u8; 4096];
            loop {
                let n = reader.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                peer.send(b"200\r\n".to_vec()).await?;
            }
            Ok(())
        })
        .build()
        .await;

    server.start_block(()).await?;
    Ok(())
}
```

## API 概览

### `Builder`

构建服务端的入口。所有方法消耗 `self` 并返回 `Self`，支持链式调用。

| 方法 | 必选 | 描述 |
|---|---|---|
| `Builder::new(addr)` | ✓ | 绑定地址（任何实现了 `ToSocketAddrs` 的类型） |
| `.set_stream_init(fn)` | ✓ | 将原始 `TcpStream` 转换为最终流类型 `C` |
| `.set_input_event(fn)` | ✓ | 每个连接调用一次的异步处理函数，参数为 `(ReadHalf<C>, Arc<TCPPeer<C>>, T)` |
| `.set_connect_event(fn)` | — | 返回 `true` 接受连接，`false` 拒绝 |
| `.build().await` | ✓ | 消耗 Builder 并返回 `Arc<dyn ITCPServer<T>>` |

### `ITCPServer<T>`

```rust
pub trait ITCPServer<T> {
    /// 开始接受连接；返回 JoinHandle。
    async fn start(&self, token: T) -> anyhow::Result<JoinHandle<anyhow::Result<()>>>;

    /// 开始接受连接并阻塞直到服务端停止。
    async fn start_block(&self, token: T) -> anyhow::Result<()>;
}
```

`T` 是任意的用户 token，会被克隆并传入每次 `input_event` 调用 —
用于在连接之间共享状态（例如 `Arc<AppState>`）。

### `TCPPeer<C>`

表示一个已连接的客户端。通过 `Arc` 可低成本克隆，多任务并发使用安全。

| 方法 | 描述 |
|---|---|
| `peer.addr()` | 远程 `SocketAddr` |
| `peer.send(buf)` | 将字节入队等待写入（缓冲，不立即 flush） |
| `peer.send_all(buf)` | 将字节入队并立即 flush |
| `peer.flush()` | 刷新写缓冲区 |
| `peer.disconnect()` | 优雅关闭连接 |
| `peer.is_disconnect()` | 检查 peer 是否已断开 |

## 架构

详见 [CLAUDE.md](CLAUDE.md)，其中包含内部架构的详细说明，包括连接生命周期、泛型参数和依赖角色。

## 相关 Crate

- **[tcpclient](https://crates.io/crates/tcpclient)** — 配套异步 TCP 客户端 crate

## 开源协议

本项目采用双许可证：

- Apache License, Version 2.0
- MIT License

任选其一。