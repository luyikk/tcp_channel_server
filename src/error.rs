use crate::State;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error(transparent)]
    JoinError(#[from] tokio::task::JoinError),
    #[error(transparent)]
    SendError(#[from] async_channel::SendError<State>),
    #[error("{0}")]
    ChannelError(String),
    #[error("not listener or repeat start")]
    NotListenerError,
}

pub type Result<T, E = Error> = core::result::Result<T, E>;
