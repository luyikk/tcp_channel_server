use crate::error::{Error, Result};
use async_channel::{Sender, TrySendError};
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
                        break;
                    }
                    State::Send(data) => {
                        if sender.write_all(&data).await.is_err() {
                            break;
                        }
                    }
                    State::SendFlush(data) => {
                        if sender.write_all(&data).await.is_err() {
                            break;
                        }
                        if sender.flush().await.is_err() {
                            break;
                        }
                    }
                    State::Flush => {
                        if sender.flush().await.is_err() {
                            break;
                        }
                    }
                }
            }

            log::debug!("TCPPeer sender task ended for {}", addr);
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

    /// 检查连接是否已断开。
    #[inline]
    fn check_disconnected(&self) -> Result<()> {
        if self.is_disconnect() {
            Err(std::io::Error::from(ErrorKind::ConnectionReset).into())
        } else {
            Ok(())
        }
    }

    /// 是否断线
    #[inline]
    pub fn is_disconnect(&self) -> bool {
        self.disconnect.load(Ordering::Relaxed)
    }

    /// 发送
    #[inline]
    pub async fn send(&self, buff: Vec<u8>) -> Result<()> {
        self.check_disconnected()?;
        match self.sender.try_send(State::Send(buff)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(state)) => Ok(self.sender.send(state).await?),
            Err(TrySendError::Closed(_)) => Err(Error::ChannelError("channel closed".to_string())),
        }
    }

    /// 发送全部
    #[inline]
    pub async fn send_all(&self, buff: Vec<u8>) -> Result<()> {
        self.check_disconnected()?;

        match self.sender.try_send(State::SendFlush(buff)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(state)) => Ok(self.sender.send(state).await?),
            Err(TrySendError::Closed(_)) => Err(Error::ChannelError("channel closed".to_string())),
        }
    }

    /// flush
    #[inline]
    pub async fn flush(&self) -> Result<()> {
        self.check_disconnected()?;
        match self.sender.try_send(State::Flush) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Ok(self.sender.send(State::Flush).await?),
            Err(TrySendError::Closed(_)) => Err(Error::ChannelError("channel closed".to_string())),
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
