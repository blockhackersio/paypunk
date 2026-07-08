use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

pub struct BridgeConfig {
    pub port: u16,
    pub socket_path: String,
}

struct BridgeState {
    pending_request: Option<Vec<u8>>,
    response_tx: Option<oneshot::Sender<Vec<u8>>>,
}

type SharedState = Arc<Mutex<BridgeState>>;

fn generate_qr_svg(bytes: &[u8]) -> String {
    let code = qrcode::QrCode::with_error_correction_level(bytes, qrcode::EcLevel::L).unwrap();
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

    let server_handle = server.handle();
    let http_handle = tokio::spawn(server);

    let accept_loop = async {
        loop {
            match listener.accept().await {
                Ok((mut stream, _addr)) => {
                    use tokio::io::AsyncReadExt;
                    let mut buf = Vec::new();
                    match stream.read_to_end(&mut buf).await {
                        Ok(_) if buf.is_empty() => continue,
                        Ok(_) => {
                            if buf.len() > 2953 {
                                use tokio::io::AsyncWriteExt;
                                let _ = stream.write_all(b"ERR: message too large").await;
                                continue;
                            }

                            let (tx, rx) = oneshot::channel();
                            {
                                let mut guard = state.lock().await;
                                guard.pending_request = Some(buf);
                                guard.response_tx = Some(tx);
                            }

                            match rx.await {
                                Ok(resp_bytes) => {
                                    use tokio::io::AsyncWriteExt;
                                    let _ = stream.write_all(&resp_bytes).await;
                                }
                                Err(_oneshot_canceled) => {}
                            }

                            let mut guard = state.lock().await;
                            guard.pending_request = None;
                            guard.response_tx = None;
                        }
                        Err(e) => {
                            eprintln!("read error: {}", e);
                        }
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

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

        let mut stream = UnixStream::connect(&socket_path).await.unwrap();
        stream.write_all(b"hello bridge").await.unwrap();
        stream.shutdown().await.unwrap();

        let status_url = format!("http://127.0.0.1:{}/status", port);
        let resp = reqwest::get(&status_url).await.unwrap();
        let status_json: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(status_json["pending"], true);
        assert_eq!(status_json["size"], 12);

        let qr_url = format!("http://127.0.0.1:{}/qr.svg", port);
        let qr_resp = reqwest::get(&qr_url).await.unwrap();
        assert_eq!(qr_resp.status(), 200);
        let svg_body = qr_resp.text().await.unwrap();
        assert!(svg_body.contains("<svg"));

        let pending_url = format!("http://127.0.0.1:{}/pending-bytes", port);
        let pb_resp = reqwest::get(&pending_url).await.unwrap();
        let pb_json: serde_json::Value = pb_resp.json().await.unwrap();
        assert_eq!(pb_json["pending"], true);
        assert_eq!(pb_json["bytes"], "aGVsbG8gYnJpZGdl");

        let response_url = format!("http://127.0.0.1:{}/response", port);
        let client = reqwest::Client::new();
        let resp_resp = client
            .post(&response_url)
            .json(&serde_json::json!({"bytes": "cmVzcG9uc2UgYnl0ZXM="}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp_resp.status(), 200);

        let mut resp_buf = Vec::new();
        stream.read_to_end(&mut resp_buf).await.unwrap();
        assert_eq!(resp_buf, b"response bytes");

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

        let mut stream = UnixStream::connect(&socket_path).await.unwrap();
        let large = vec![0u8; 3000];
        stream.write_all(&large).await.unwrap();
        stream.shutdown().await.unwrap();

        let mut resp_buf = Vec::new();
        stream.read_to_end(&mut resp_buf).await.unwrap();
        assert!(resp_buf.starts_with(b"ERR: message too large"));

        handle.abort();
    }
}
