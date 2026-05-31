use std::path::Path;

use bytes::BytesMut;
use tactix::{Actor, Addr, Handler, Sender};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

use crate::messages::IpcMessage;
use crate::router::IpcError;

// ---------------------------------------------------------------------------
// Server — listens on a Unix socket and dispatches requests
// ---------------------------------------------------------------------------

pub struct IpcServer {
    listener: UnixListener,
}

impl IpcServer {
    pub async fn bind(path: impl AsRef<Path>) -> Result<Self, IpcError> {
        let path = path.as_ref();
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        let listener = UnixListener::bind(path)?;
        Ok(Self { listener })
    }

    /// Accept incoming connections in a loop. Each connection reads
    /// a length-prefixed frame, forwards it as `IpcMessage` to the
    /// handler actor, and writes the response bytes back.
    pub async fn serve<H>(&self, handler: Addr<H>) -> Result<(), IpcError>
    where
        H: Actor + Handler<IpcMessage>,
    {
        loop {
            let (stream, _) = self.listener.accept().await?;
            let handler = handler.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, handler).await {
                    eprintln!("IPC connection error: {e}");
                }
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Per-connection handler
// ---------------------------------------------------------------------------

async fn handle_connection<H>(mut stream: UnixStream, handler: Addr<H>) -> Result<(), IpcError>
where
    H: Actor + Handler<IpcMessage>,
{
    let mut read_buf = BytesMut::with_capacity(4096);

    loop {
        let mut len_buf = [0u8; 4];
        if stream.read_exact(&mut len_buf).await.is_err() {
            return Ok(());
        }
        let len = u32::from_le_bytes(len_buf) as usize;
        read_buf.resize(len, 0);
        stream.read_exact(&mut read_buf[..len]).await?;

        let request_bytes = read_buf[..len].to_vec();
        let response = handler.ask(IpcMessage(request_bytes)).await;

        let (status, payload) = match response {
            Ok(bytes) => (0u8, bytes),
            Err(e) => (1u8, e.into_bytes()),
        };

        let len = (payload.len() + 1) as u32;
        stream.write_all(&len.to_le_bytes()).await?;
        stream.write_all(&[status]).await?;
        stream.write_all(&payload).await?;
        stream.flush().await?;
    }
}
