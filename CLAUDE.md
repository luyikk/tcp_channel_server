# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test

```bash
# Build
cargo build

# Build with TLS support
cargo build --features tls

# Run all tests
cargo test

# Run all tests with TLS enabled
cargo test --features tls

# Run a single test
cargo test -- test_builder

# Run examples
cargo run --example echo_server
cargo run --example ssl_server --features tls
```

## Architecture

This is a lightweight async TCP server framework built on Tokio, published as `tcp-channel-server` on crates.io.

### Core types and their roles

- **`Builder`** (`src/builder.rs`) — Fluent builder with phantom types tracking all generics (`I`, `C`, `T`, etc.). Required calls: `set_stream_init` and `set_input_event`. Optional: `set_connect_event`. `build()` returns `Arc<Actor<TCPServer<...>>>`, panicking if required setters were skipped.

- **`TCPServer`** (`src/tcpserver.rs`) — Wraps a `TcpListener` inside `aqueue::Actor` for lock-free interior mutability. On `start()`, it takes ownership of the listener (one-shot — calling start again returns `NotListenerError`) and spawns an accept loop. Each accepted connection spawns a `handle_connection` task.

- **`ITCPServer<T>` trait** — Public trait with `start()` (returns `JoinHandle`) and `start_block()` (blocks until the server ends). Implemented on `Actor<TCPServer<...>>`.

- **`TCPPeer<C>`** (`src/peer.rs`) — Per-connection handle. It holds a `Sender<State>` into a bounded `async_channel` (capacity 4096). A dedicated writer task receives `State` variants (`Send`, `SendFlush`, `Flush`, `Disconnect`) and executes them on the stream's `WriteHalf`. This channels writes into a single async task, eliminating locks on the write side.

- **`Error`** (`src/error.rs`) — Crate-level error enum using `thiserror`, wrapping IO errors, join errors, channel send errors, and a `NotListenerError` for double-start attempts.

### Connection lifecycle

1. `TcpListener::accept` yields a raw `TcpStream`.
2. Optional `connect_event` closure filters by `SocketAddr` — return `false` to reject.
3. `stream_init` closure transforms `TcpStream` → `C` (identity for plain TCP; `SslStream` for TLS).
4. `tokio::io::split` splits `C` into `ReadHalf` (given to user) and `WriteHalf` (moved into the `TCPPeer` writer task).
5. The user's `input_event` closure is called with `(ReadHalf<C>, Arc<TCPPeer<C>>, T)` and runs until it returns or the peer disconnects.
6. On return, `peer.disconnect()` is called automatically to shut down the writer task.

### Token (`T`)

The generic `T: Clone + Send + 'static` is a user-provided token cloned into every `input_event` invocation. Use it to share state (e.g., `Arc<AppState>`) across all connections without globals.

### TLS (`tls` feature flag)

Gates `openssl`, `openssl-sys`, and `tokio-openssl`. When enabled, `stream_init` wraps a `TcpStream` in `tokio_openssl::SslStream`. The framework itself has no TLS-specific code — it's entirely driven by the user's `stream_init` closure.

### Key dependencies

- **`aqueue`** — Provides `Actor<T>`, a lock-free wrapper for `!Send` types that need to be shared across tasks. `TCPServer` is `!Send` (holds `PhantomData` over function pointers) so `Actor` makes it usable from `Arc`.
- **`async-channel`** — MPSC channel used to queue write operations to the per-connection writer task.
- **`tokio`** — Runtime, TCP listener, I/O utilities (`split`, `AsyncRead`, `AsyncWrite`).

### Crate versioning note

The README examples use version `"0.2"` but the current crate is `0.3.1`. The API is the same; the version bump added the `thiserror`-based error enum.
