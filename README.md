# tcp-channel-server

A lightweight, async TCP server framework for Rust built on top of [Tokio](https://tokio.rs/).

[![Crates.io](https://img.shields.io/crates/v/tcp-channel-server.svg)](https://crates.io/crates/tcp-channel-server)
[![Documentation](https://docs.rs/tcp_channel_server/badge.svg)](https://docs.rs/tcp_channel_server)
[![License](https://img.shields.io/crates/l/tcp-channel-server.svg)](LICENSE)

## Features

- Async TCP server powered by Tokio
- Fluent builder API for easy configuration
- Per-connection stream initialization hook (plain TCP, TLS, or any custom wrapper)
- Optional connection filter callback to accept or reject incoming peers
- Channel-based peer abstraction for safe, concurrent sends
- TLS support via `openssl` (opt-in feature flag)

## Requirements

- Rust **1.75** or later (edition 2021)

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
tcp-channel-server = "0.2"
tokio = { version = "1", features = ["full"] }
```

To enable TLS support:

```toml
[dependencies]
tcp-channel-server = { version = "0.2", features = ["tls"] }
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

## Related Crates

- **[tcpclient](https://crates.io/crates/tcpclient)** — Async TCP client companion crate

## License

Licensed under either of

- Apache License, Version 2.0
- MIT License

at your option.
