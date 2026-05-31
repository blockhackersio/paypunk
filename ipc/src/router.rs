use bytes::BytesMut;
use tactix::{Actor, Addr, Ctx, Handler};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::messages::IpcMessage;

/// Error type for IPC transport operations.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Connection closed")]
    ConnectionClosed,
}

// ---------------------------------------------------------------------------
// IpcActor — wraps a UnixStream as a tactix actor
// ---------------------------------------------------------------------------

pub struct IpcActor {
    stream: UnixStream,
    read_buf: BytesMut,
}

impl IpcActor {
    pub fn from_stream(stream: UnixStream) -> Self {
        Self {
            stream,
            read_buf: BytesMut::with_capacity(4096),
        }
    }

    /// Connect to a Unix socket at `path` and return a running actor address.
    pub async fn connect(path: &str) -> Result<Addr<Self>, IpcError> {
        let stream = UnixStream::connect(path).await?;
        Ok(Self::from_stream(stream).start())
    }

    /// Read a length-prefixed frame from the socket.
    async fn read_raw(&mut self) -> Result<Vec<u8>, IpcError> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_le_bytes(len_buf) as usize;
        self.read_buf.resize(len, 0);
        self.stream.read_exact(&mut self.read_buf[..len]).await?;
        Ok(self.read_buf[..len].to_vec())
    }

    /// Write a length-prefixed frame to the socket.
    async fn write_raw(&mut self, data: &[u8]) -> Result<(), IpcError> {
        let len = data.len() as u32;
        self.stream.write_all(&len.to_le_bytes()).await?;
        self.stream.write_all(data).await?;
        self.stream.flush().await?;
        Ok(())
    }
}

impl Actor for IpcActor {}

impl Handler<IpcMessage> for IpcActor {
    async fn handle(&mut self, msg: IpcMessage, _ctx: &Ctx<Self>) -> Result<Vec<u8>, String> {
        self.write_raw(&msg.0).await.map_err(|e| e.to_string())?;
        let raw = self.read_raw().await.map_err(|e| e.to_string())?;
        if raw.is_empty() {
            return Err("empty response".into());
        }
        match raw[0] {
            0 => Ok(raw[1..].to_vec()),
            1 => {
                let msg = String::from_utf8_lossy(&raw[1..]).to_string();
                Err(msg)
            }
            _ => Err("invalid response status".into()),
        }
    }
}
