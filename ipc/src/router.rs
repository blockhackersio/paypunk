use blake2::digest::consts::U32;
use blake2::Digest;
use bytes::BytesMut;
use rand::RngCore;
use tactix::{Actor, Addr, Ctx, Handler};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::messages::{
    IpcMessage, MAC_LEN, MSG_APPLICATION, MSG_GET_PUBLIC_KEY, MSG_PUBLIC_KEY, MSG_REGISTER_CLIENT,
    MSG_REGISTER_CLIENT_ACK,
};

/// Error type for IPC transport operations.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Handshake failed: {0}")]
    HandshakeFailed(String),
}

// ---------------------------------------------------------------------------
// Helpers
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
// IpcSender — wraps a UnixStream as a tactix actor
// ---------------------------------------------------------------------------

pub struct IpcSender {
    stream: UnixStream,
    read_buf: BytesMut,
    hmac_key: [u8; 32],
}

impl IpcSender {
    /// Connect to a Unix socket at `path`, perform the authenticated
    /// handshake, and return a running actor address.
    ///
    /// The handshake is transparent to the caller:
    /// 1. Generate an ephemeral X25519 keypair for this connection
    /// 2. Request the server's public key
    /// 3. Register our public key with the server
    /// 4. Derive a shared HMAC key for message authentication
    pub async fn connect(path: &str) -> Result<Addr<Self>, IpcError> {
        let mut stream = UnixStream::connect(path).await?;
        let mut buf = BytesMut::with_capacity(4096);

        // Generate our keypair
        let (client_secret, client_public) = generate_keypair();

        // Step 1: Send GetPublicKey
        write_frame(&mut stream, &[MSG_GET_PUBLIC_KEY]).await?;

        // Step 2: Receive server's public key
        let frame = read_frame(&mut stream, &mut buf).await?;
        if frame.len() != 33 || frame[0] != MSG_PUBLIC_KEY {
            return Err(IpcError::HandshakeFailed(
                "unexpected response to GetPublicKey".into(),
            ));
        }
        let mut server_public = [0u8; 32];
        server_public.copy_from_slice(&frame[1..33]);

        // Step 3: Send RegisterClient with our public key
        let mut reg = vec![MSG_REGISTER_CLIENT];
        reg.extend_from_slice(&client_public);
        write_frame(&mut stream, &reg).await?;

        // Step 4: Wait for registration ack
        let ack = read_frame(&mut stream, &mut buf).await?;
        if ack.len() != 1 || ack[0] != MSG_REGISTER_CLIENT_ACK {
            return Err(IpcError::HandshakeFailed(
                "client registration rejected".into(),
            ));
        }

        // Derive shared HMAC key
        let shared = x25519_dalek::x25519(client_secret, server_public);
        let hmac_key = compute_mac(&shared, b"paypunk-ipc-hmac");

        let actor = Self {
            stream,
            read_buf: BytesMut::with_capacity(4096),
            hmac_key,
        };

        Ok(actor.start())
    }

    async fn read_raw(&mut self) -> Result<Vec<u8>, IpcError> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_le_bytes(len_buf) as usize;
        self.read_buf.resize(len, 0);
        self.stream.read_exact(&mut self.read_buf[..len]).await?;
        Ok(self.read_buf[..len].to_vec())
    }

    async fn write_raw(&mut self, data: &[u8]) -> Result<(), IpcError> {
        let len = data.len() as u32;
        self.stream.write_all(&len.to_le_bytes()).await?;
        self.stream.write_all(data).await?;
        self.stream.flush().await?;
        Ok(())
    }
}

impl Actor for IpcSender {}

impl Handler<IpcMessage> for IpcSender {
    async fn handle(&mut self, msg: IpcMessage, _ctx: &Ctx<Self>) -> Result<Vec<u8>, String> {
        // Build application frame: type byte + payload + MAC
        let mac = compute_mac(&self.hmac_key, &msg.0);
        let mut frame = Vec::with_capacity(1 + msg.0.len() + MAC_LEN);
        frame.push(MSG_APPLICATION);
        frame.extend_from_slice(&msg.0);
        frame.extend_from_slice(&mac);

        self.write_raw(&frame).await.map_err(|e| e.to_string())?;

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

// ---------------------------------------------------------------------------
// Frame I/O helpers (duplicated from server to keep router self-contained)
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
