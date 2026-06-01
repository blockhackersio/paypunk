use std::path::Path;

use blake2::digest::consts::U32;
use blake2::Digest;
use bytes::BytesMut;
use rand::RngCore;
use tactix::{Actor, Addr, Handler, Sender};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

use crate::messages::{
    IpcMessage, APPROVE_CONNECTION, MAC_LEN, MSG_APPLICATION, MSG_GET_PUBLIC_KEY, MSG_PUBLIC_KEY,
    MSG_REGISTER_CLIENT, MSG_REGISTER_CLIENT_ACK,
};
use crate::router::IpcError;

// ---------------------------------------------------------------------------
// Keypair generation (X25519)
// ---------------------------------------------------------------------------

fn generate_keypair() -> ([u8; 32], [u8; 32]) {
    let mut secret = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut secret);
    secret[0] &= 248;
    secret[31] &= 127;
    secret[31] |= 64;
    let public = x25519_dalek::x25519(secret, x25519_dalek::X25519_BASEPOINT_BYTES);
    (secret, public)
}

fn compute_mac(key: &[u8; 32], message: &[u8]) -> [u8; 32] {
    let mut hasher = blake2::Blake2b::<U32>::new();
    hasher.update(key);
    hasher.update(message);
    let result = hasher.finalize();
    let mut mac = [0u8; 32];
    mac.copy_from_slice(&result);
    mac
}

// ---------------------------------------------------------------------------
// Per-connection auth state
// ---------------------------------------------------------------------------

struct ConnectionAuth {
    hmac_key: Option<[u8; 32]>,
    registered: bool,
    approved: bool,
}

impl ConnectionAuth {
    fn new() -> Self {
        Self {
            hmac_key: None,
            registered: false,
            approved: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Server — listens on a Unix socket and dispatches requests
// ---------------------------------------------------------------------------

pub struct IpcReceiver {
    listener: UnixListener,
    secret: [u8; 32],
    public: [u8; 32],
}

impl IpcReceiver {
    /// Create a server with an existing listener and keypair.
    /// Used when the caller wants to control the keypair (e.g., share
    /// keypunkd's KeyStore keypair so the handshake key matches the
    /// encryption key).
    pub fn new(listener: UnixListener, secret: [u8; 32], public: [u8; 32]) -> Self {
        Self {
            listener,
            secret,
            public,
        }
    }

    pub async fn bind(path: impl AsRef<Path>) -> Result<Self, IpcError> {
        let path = path.as_ref();
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        let listener = UnixListener::bind(path)?;
        let (secret, public) = generate_keypair();
        Ok(Self {
            listener,
            secret,
            public,
        })
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.public
    }

    /// Accept incoming connections. Each connection runs the handshake,
    /// then reads authenticated application messages and dispatches them
    /// to the handler actor.
    pub async fn serve<H>(&self, handler: Addr<H>) -> Result<(), IpcError>
    where
        H: Actor + Handler<IpcMessage>,
    {
        loop {
            let (stream, _) = self.listener.accept().await?;
            let handler = handler.clone();
            let secret = self.secret;
            let public = self.public;
            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, handler, secret, public).await {
                    eprintln!("IPC connection error: {e}");
                }
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Frame read/write helpers
// ---------------------------------------------------------------------------

async fn read_frame(stream: &mut UnixStream, buf: &mut BytesMut) -> Result<Vec<u8>, IpcError> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    buf.resize(len, 0);
    stream.read_exact(&mut buf[..len]).await?;
    Ok(buf[..len].to_vec())
}

async fn write_frame(stream: &mut UnixStream, data: &[u8]) -> Result<(), IpcError> {
    let len = data.len() as u32;
    stream.write_all(&len.to_le_bytes()).await?;
    stream.write_all(data).await?;
    stream.flush().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Per-connection handler
// ---------------------------------------------------------------------------

async fn handle_connection<H>(
    mut stream: UnixStream,
    handler: Addr<H>,
    secret: [u8; 32],
    public: [u8; 32],
) -> Result<(), IpcError>
where
    H: Actor + Handler<IpcMessage>,
{
    let mut buf = BytesMut::with_capacity(4096);
    let mut auth = ConnectionAuth::new();

    loop {
        let frame = read_frame(&mut stream, &mut buf).await?;
        if frame.is_empty() {
            return Ok(());
        }

        let msg_type = frame[0];
        let payload = &frame[1..];

        match msg_type {
            MSG_GET_PUBLIC_KEY => {
                let mut response = vec![MSG_PUBLIC_KEY];
                response.extend_from_slice(&public);
                write_frame(&mut stream, &response).await?;
            }

            MSG_REGISTER_CLIENT => {
                if payload.len() != 32 {
                    return Ok(()); // invalid, drop connection
                }
                let mut client_pk = [0u8; 32];
                client_pk.copy_from_slice(payload);
                let shared = x25519_dalek::x25519(secret, client_pk);
                let hmac_key = compute_mac(&shared, b"paypunk-ipc-hmac");
                auth.hmac_key = Some(hmac_key);
                auth.registered = true;
                write_frame(&mut stream, &[MSG_REGISTER_CLIENT_ACK]).await?;
            }

            MSG_APPLICATION => {
                if !auth.registered {
                    return Ok(()); // must register first
                }
                if payload.len() < MAC_LEN {
                    return Ok(()); // malformed
                }
                let (msg_payload, msg_mac) = payload.split_at(payload.len() - MAC_LEN);
                let hmac_key = auth.hmac_key.as_ref().unwrap();
                let expected_mac = compute_mac(hmac_key, msg_payload);
                if msg_mac != expected_mac {
                    return Ok(()); // MAC mismatch, drop connection
                }

                // Forward to handler
                let response = handler.ask(IpcMessage(msg_payload.to_vec())).await;

                match response {
                    Ok(bytes) => {
                        if !bytes.is_empty() && bytes[0] == APPROVE_CONNECTION {
                            auth.approved = true;
                            let len = (bytes.len() - 1 + 1) as u32; // +1 for status
                            stream.write_all(&len.to_le_bytes()).await?;
                            stream.write_all(&[0u8]).await?; // status 0 = success
                            stream.write_all(&bytes[1..]).await?;
                        } else {
                            let len = (bytes.len() + 1) as u32;
                            stream.write_all(&len.to_le_bytes()).await?;
                            stream.write_all(&[0u8]).await?;
                            stream.write_all(&bytes).await?;
                        }
                        stream.flush().await?;
                    }
                    Err(e) => {
                        let err_bytes = e.into_bytes();
                        let len = (err_bytes.len() + 1) as u32;
                        stream.write_all(&len.to_le_bytes()).await?;
                        stream.write_all(&[1u8]).await?; // status 1 = error
                        stream.write_all(&err_bytes).await?;
                        stream.flush().await?;
                    }
                }
            }

            _ => {
                return Ok(()); // unknown message type, drop connection
            }
        }
    }
}
