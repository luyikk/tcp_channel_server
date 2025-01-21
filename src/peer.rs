use crate::error::Result;
use async_channel::Sender;
use std::io::ErrorKind;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::WriteHalf;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

pub enum State {
    Disconnect,
    Send(Vec<u8>),
    SendFlush(Vec<u8>),
    Flush,
}

pub struct TCPPeer<T> {
    pub addr: SocketAddr,
    pub sender: Sender<State>,
    disconnect: AtomicBool,
    _ph: PhantomData<T>,
}

impl<T> TCPPeer<T>
where
    T: AsyncRead + AsyncWrite + Send + 'static,
{
    /// 创建一个TCP PEER
    #[inline]
    pub fn new(addr: SocketAddr, mut sender: WriteHalf<T>) -> Arc<TCPPeer<T>> {
        let (tx, rx) = async_channel::bounded(4096);

        tokio::spawn(async move {
            while let Ok(state) = rx.recv().await {
                match state {
                    State::Disconnect => {
                        let _ = sender.shutdown().await;
                        return;
                    }
                    State::Send(data) => {
                        if sender.write(&data).await.is_err() {
                            return;
                        }
                    }
                    State::SendFlush(data) => {
                        if sender.write(&data).await.is_err() {
                            return;
                        }
                        if sender.flush().await.is_err() {
                            return;
                        }
                    }
                    State::Flush => {
                        if sender.flush().await.is_err() {
                            return;
                        }
                    }
                }
            }
        });

        Arc::new(TCPPeer {
            addr,
            sender: tx,
            disconnect: AtomicBool::new(false),
            _ph: Default::default(),
        })
    }

    /// ipaddress
    #[inline]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// 是否断线
    #[inline]
    pub fn is_disconnect(&self) -> bool {
        self.disconnect.load(Ordering::Acquire)
    }

    /// 发送
    #[inline]
    pub async fn send(&self, buff: Vec<u8>) -> Result<()> {
        if !self.disconnect.load(Ordering::Acquire) {
            Ok(self.sender.clone().send(State::Send(buff)).await?)
        } else {
            Err(std::io::Error::from(ErrorKind::ConnectionReset).into())
        }
    }

    /// 发送全部
    #[inline]
    pub async fn send_all(&self, buff: Vec<u8>) -> Result<()> {
        if !self.disconnect.load(Ordering::Acquire) {
            Ok(self.sender.clone().send(State::SendFlush(buff)).await?)
        } else {
            Err(std::io::Error::from(ErrorKind::ConnectionReset).into())
        }
    }

    /// flush
    #[inline]
    pub async fn flush(&self) -> Result<()> {
        if !self.disconnect.load(Ordering::Acquire) {
            Ok(self.sender.send(State::Flush).await?)
        } else {
            Err(std::io::Error::from(ErrorKind::ConnectionReset).into())
        }
    }

    /// 掐线
    #[inline]
    pub async fn disconnect(&self) -> Result<()> {
        if !self.disconnect.load(Ordering::Acquire) {
            self.sender.send(State::Disconnect).await?;
            self.disconnect.store(true, Ordering::Release);
        }
        Ok(())
    }
}
