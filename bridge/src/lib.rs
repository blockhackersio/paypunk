use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

use blake2::digest::consts::U32;
use blake2::Digest;
use paypunk_ipc::messages::{
    MAC_LEN, MSG_APPLICATION, MSG_GET_PUBLIC_KEY, MSG_PUBLIC_KEY, MSG_REGISTER_CLIENT,
    MSG_REGISTER_CLIENT_ACK,
};
use paypunk_ipc::transport::UnixSocketTransport;
use rand::RngCore;

pub struct BridgeConfig {
    pub port: u16,
    pub socket_path: String,
}

struct BridgeState {
    pending_request: Option<Vec<u8>>,
    response_tx: Option<oneshot::Sender<Vec<u8>>>,
}

type SharedState = Arc<Mutex<BridgeState>>;

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
    let mut mac = [0u8; 32];
    mac.copy_from_slice(&hasher.finalize());
    mac
}

fn generate_qr_svg(bytes: &[u8]) -> String {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    let code =
        qrcode::QrCode::with_error_correction_level(b64.as_bytes(), qrcode::EcLevel::L).unwrap();
    code.render()
        .min_dimensions(400, 400)
        .dark_color(qrcode::render::svg::Color("#000000"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build()
}

use actix_web::{get, post, web, HttpResponse};

#[get("/")]
async fn index() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html")
        .body(include_str!("bridge.html"))
}

#[get("/jsqr.js")]
async fn jsqr() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/javascript")
        .body(include_bytes!("jsqr.js").as_ref())
}

#[get("/status")]
async fn status(state: web::Data<SharedState>) -> HttpResponse {
    let guard = state.lock().await;
    if let Some(ref req) = guard.pending_request {
        HttpResponse::Ok().json(serde_json::json!({
            "pending": true,
            "size": req.len()
        }))
    } else {
        HttpResponse::Ok().json(serde_json::json!({
            "pending": false
        }))
    }
}

#[get("/qr.svg")]
async fn qr_svg(state: web::Data<SharedState>) -> HttpResponse {
    let guard = state.lock().await;
    match guard.pending_request.as_ref() {
        Some(bytes) => {
            let svg = generate_qr_svg(bytes);
            HttpResponse::Ok().content_type("image/svg+xml").body(svg)
        }
        None => HttpResponse::NoContent().finish(),
    }
}

#[get("/pending-bytes")]
async fn pending_bytes(state: web::Data<SharedState>) -> HttpResponse {
    let guard = state.lock().await;
    match guard.pending_request.as_ref() {
        Some(bytes) => {
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
            HttpResponse::Ok().json(serde_json::json!({
                "pending": true,
                "bytes": b64
            }))
        }
        None => HttpResponse::Ok().json(serde_json::json!({
            "pending": false
        })),
    }
}

#[derive(serde::Deserialize)]
struct ResponseBody {
    bytes: String,
}

#[post("/response")]
async fn response(state: web::Data<SharedState>, body: web::Json<ResponseBody>) -> HttpResponse {
    use base64::Engine;
    let decoded = match base64::engine::general_purpose::STANDARD.decode(&body.bytes) {
        Ok(d) => d,
        Err(_) => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": "invalid base64"
            }))
        }
    };

    let mut guard = state.lock().await;
    match guard.response_tx.take() {
        Some(tx) => {
            let _ = tx.send(decoded);
            guard.pending_request = None;
            HttpResponse::Ok().json(serde_json::json!({
                "status": "ok"
            }))
        }
        None => HttpResponse::BadRequest().json(serde_json::json!({
            "error": "no pending request"
        })),
    }
}

pub async fn run(config: BridgeConfig) -> Result<(), Box<dyn std::error::Error>> {
    let _ = std::fs::remove_file(&config.socket_path);

    let state: SharedState = Arc::new(Mutex::new(BridgeState {
        pending_request: None,
        response_tx: None,
    }));

    let listener = tokio::net::UnixListener::bind(&config.socket_path)?;

    let accept_state = state.clone();
    let server = actix_web::HttpServer::new(move || {
        actix_web::App::new()
            .app_data(web::Data::new(accept_state.clone()))
            .service(index)
            .service(jsqr)
            .service(status)
            .service(qr_svg)
            .service(pending_bytes)
            .service(response)
    })
    .bind(format!("0.0.0.0:{}", config.port))?
    .run();
    println!(
        "\nBridge Server available at:\n\nhttp://127.0.0.1:{}\n\n",
        config.port
    );
    let server_handle = server.handle();
    let http_handle = tokio::spawn(server);

    let (secret, public) = generate_keypair();

    let accept_loop = async {
        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let state = state.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_ipc_connection(stream, state, secret, public).await {
                            eprintln!("bridge connection error: {e}");
                        }
                    });
                }
                Err(e) => {
                    eprintln!("accept error: {}", e);
                }
            }
        }
    };

    tokio::select! {
        _ = accept_loop => {},
        _ = tokio::signal::ctrl_c() => {
            println!("\nshutting down...");
        }
    }

    server_handle.stop(true).await;
    let _ = http_handle.await;
    let _ = std::fs::remove_file(&config.socket_path);

    Ok(())
}

async fn handle_ipc_connection(
    stream: tokio::net::UnixStream,
    state: SharedState,
    secret: [u8; 32],
    public: [u8; 32],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut transport = UnixSocketTransport::from_stream(stream);
    let mut hmac_key: Option<[u8; 32]> = None;
    let mut registered = false;

    loop {
        let frame = transport.read_frame().await?;
        if frame.is_empty() {
            return Ok(());
        }

        let msg_type = frame[0];
        let payload = &frame[1..];

        match msg_type {
            MSG_GET_PUBLIC_KEY => {
                let mut resp = vec![MSG_PUBLIC_KEY];
                resp.extend_from_slice(&public);
                transport.write_frame(&resp).await?;
            }

            MSG_REGISTER_CLIENT => {
                if payload.len() != 32 {
                    return Err("invalid client public key length".into());
                }
                let mut client_pk = [0u8; 32];
                client_pk.copy_from_slice(payload);
                let shared = x25519_dalek::x25519(secret, client_pk);
                hmac_key = Some(compute_mac(&shared, b"paypunk-ipc-hmac"));
                registered = true;
                transport.write_frame(&[MSG_REGISTER_CLIENT_ACK]).await?;
            }

            MSG_APPLICATION => {
                if !registered {
                    return Err("application message before registration".into());
                }
                if payload.len() < MAC_LEN {
                    return Err("malformed application message".into());
                }
                let (msg_payload, msg_mac) = payload.split_at(payload.len() - MAC_LEN);
                let expected_mac = compute_mac(hmac_key.as_ref().unwrap(), msg_payload);
                if msg_mac != expected_mac {
                    return Err("MAC mismatch".into());
                }

                if msg_payload.len() > 2953 {
                    let mut resp = vec![1u8];
                    println!("Message too large: {}", msg_payload.len());
                    resp.extend_from_slice(b"message too large.");
                    transport.write_frame(&resp).await?;
                    return Ok(());
                }

                let (tx, rx) = oneshot::channel();
                {
                    let mut guard = state.lock().await;
                    guard.pending_request = Some(frame.to_vec());
                    guard.response_tx = Some(tx);
                }

                match rx.await {
                    Ok(resp_bytes) => {
                        transport.write_frame(&resp_bytes).await?;
                    }
                    Err(_) => {
                        let mut frame = vec![1u8];
                        frame.extend_from_slice(b"request cancelled");
                        transport.write_frame(&frame).await?;
                    }
                }

                let mut guard = state.lock().await;
                guard.pending_request = None;
                guard.response_tx = None;

                continue;
            }

            _ => {
                eprintln!("unknown IPC message type: {msg_type}");
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paypunk_ipc::messages::MSG_APPLICATION;
    use paypunk_ipc::transport::UnixSocketTransport;
    use tokio::net::UnixStream;

    async fn ipc_handshake(transport: &mut UnixSocketTransport) -> ([u8; 32], [u8; 32]) {
        transport.write_frame(&[MSG_GET_PUBLIC_KEY]).await.unwrap();

        let frame = transport.read_frame().await.unwrap();
        assert_eq!(frame.len(), 33);
        assert_eq!(frame[0], MSG_PUBLIC_KEY);
        let mut server_public = [0u8; 32];
        server_public.copy_from_slice(&frame[1..33]);

        let (client_secret, client_public) = generate_keypair();

        let mut reg = vec![MSG_REGISTER_CLIENT];
        reg.extend_from_slice(&client_public);
        transport.write_frame(&reg).await.unwrap();

        let ack = transport.read_frame().await.unwrap();
        assert_eq!(ack, vec![MSG_REGISTER_CLIENT_ACK]);

        let shared = x25519_dalek::x25519(client_secret, server_public);
        let hmac_key = compute_mac(&shared, b"paypunk-ipc-hmac");

        (server_public, hmac_key)
    }

    async fn send_application_msg(
        transport: &mut UnixSocketTransport,
        hmac_key: &[u8; 32],
        payload: &[u8],
    ) {
        let mac = compute_mac(hmac_key, payload);
        let mut frame = Vec::with_capacity(1 + payload.len() + MAC_LEN);
        frame.push(MSG_APPLICATION);
        frame.extend_from_slice(payload);
        frame.extend_from_slice(&mac);
        transport.write_frame(&frame).await.unwrap();
    }

    async fn read_application_response(
        transport: &mut UnixSocketTransport,
    ) -> Result<Vec<u8>, String> {
        let raw = transport.read_frame().await.unwrap();
        match raw[0] {
            0 => Ok(raw[1..].to_vec()),
            1 => Err(String::from_utf8_lossy(&raw[1..]).to_string()),
            _ => Err("invalid status".into()),
        }
    }

    #[tokio::test]
    async fn test_bridge_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("test.sock").to_string_lossy().to_string();
        let port = 18444u16;

        let config = BridgeConfig {
            port,
            socket_path: socket_path.clone(),
        };

        let handle = tokio::spawn(async move {
            run(config).await.unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let stream = UnixStream::connect(&socket_path).await.unwrap();
        let mut transport = UnixSocketTransport::from_stream(stream);

        let (_server_public, hmac_key) = ipc_handshake(&mut transport).await;

        send_application_msg(&mut transport, &hmac_key, b"hello bridge").await;

        let status_url = format!("http://127.0.0.1:{}/status", port);
        let resp = reqwest::get(&status_url).await.unwrap();
        let status_json: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(status_json["pending"], true);
        assert_eq!(status_json["size"], 1 + 12 + MAC_LEN);

        let qr_url = format!("http://127.0.0.1:{}/qr.svg", port);
        let qr_resp = reqwest::get(&qr_url).await.unwrap();
        assert_eq!(qr_resp.status(), 200);
        let svg_body = qr_resp.text().await.unwrap();
        assert!(svg_body.contains("<svg"));

        let pending_url = format!("http://127.0.0.1:{}/pending-bytes", port);
        let pb_resp = reqwest::get(&pending_url).await.unwrap();
        let pb_json: serde_json::Value = pb_resp.json().await.unwrap();
        assert_eq!(pb_json["pending"], true);
        {
            use base64::Engine;
            let mut frame = vec![MSG_APPLICATION];
            frame.extend_from_slice(b"hello bridge");
            let mac = compute_mac(&hmac_key, b"hello bridge");
            frame.extend_from_slice(&mac);
            let expected_b64 = base64::engine::general_purpose::STANDARD.encode(&frame);
            assert_eq!(pb_json["bytes"], expected_b64);
        }

        let response_url = format!("http://127.0.0.1:{}/response", port);
        let client = reqwest::Client::new();
        let resp_resp = client
            .post(&response_url)
            .json(&serde_json::json!({"bytes": "AHJlc3BvbnNlIGJ5dGVz"}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp_resp.status(), 200);

        let resp_bytes = read_application_response(&mut transport).await.unwrap();
        assert_eq!(resp_bytes, b"response bytes");

        handle.abort();
    }

    #[tokio::test]
    async fn test_no_pending_returns_400() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("test2.sock").to_string_lossy().to_string();
        let port = 18445u16;

        let config = BridgeConfig {
            port,
            socket_path: socket_path.clone(),
        };

        let handle = tokio::spawn(async move {
            run(config).await.unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let response_url = format!("http://127.0.0.1:{}/response", port);
        let client = reqwest::Client::new();
        let resp = client
            .post(&response_url)
            .json(&serde_json::json!({"bytes": "dGVzdA=="}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400);

        let status_url = format!("http://127.0.0.1:{}/status", port);
        let resp = reqwest::get(&status_url).await.unwrap();
        let json: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(json["pending"], false);

        handle.abort();
    }

    #[tokio::test]
    async fn test_qr_svg_returns_204_when_idle() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("test3.sock").to_string_lossy().to_string();
        let port = 18446u16;

        let config = BridgeConfig {
            port,
            socket_path: socket_path.clone(),
        };

        let handle = tokio::spawn(async move {
            run(config).await.unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let qr_url = format!("http://127.0.0.1:{}/qr.svg", port);
        let resp = reqwest::get(&qr_url).await.unwrap();
        assert_eq!(resp.status(), 204);

        handle.abort();
    }

    #[tokio::test]
    async fn test_message_too_large() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("test4.sock").to_string_lossy().to_string();
        let port = 18447u16;

        let config = BridgeConfig {
            port,
            socket_path: socket_path.clone(),
        };

        let handle = tokio::spawn(async move {
            run(config).await.unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let stream = UnixStream::connect(&socket_path).await.unwrap();
        let mut transport = UnixSocketTransport::from_stream(stream);

        let (_server_public, hmac_key) = ipc_handshake(&mut transport).await;

        let large = vec![0u8; 3000];
        send_application_msg(&mut transport, &hmac_key, &large).await;

        let err = read_application_response(&mut transport).await.unwrap_err();
        assert_eq!(err, "message too large");

        handle.abort();
    }
}
