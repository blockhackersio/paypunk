# Step: Implement the bridge core

Implement the full bridge logic: Unix socket listener, actix-web HTTP server, shared state coordination, and the HTML page with webcam QR scanning.

**This step is purely additive.** No existing code outside `bridge/` is modified. Only files in the `bridge/` crate are created or changed.

## Tasks

### 1. Create types in `bridge/src/lib.rs`

```rust
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};

pub struct BridgeConfig {
    pub port: u16,
    pub socket_path: String,
}

struct BridgeState {
    pending_request: Option<Vec<u8>>,
    response_tx: Option<oneshot::Sender<Vec<u8>>>,
}

type SharedState = Arc<Mutex<BridgeState>>;
```

### 2. Implement the `run` function

The `run` function should:
1. Remove any stale socket file at `config.socket_path`
2. Create the shared state
3. Bind a Unix socket listener at `config.socket_path`
4. Spawn the actix-web HTTP server on `config.port` (in a separate task)
5. Run the Unix socket accept loop sequentially (blocking, on the main task)
6. On Ctrl+C, clean up the socket file and exit

Use `tokio::select!` or spawn the HTTP server into a background task while the Unix socket loop runs on the main task.

### 3. Unix socket listener loop

Sequential loop — one request at a time:

```
loop {
    accept one connection
    read all bytes from client (use tokio::io::AsyncReadExt::read_to_end)
    if bytes.len() > 2953:
        write error message to client, close connection, continue
    store bytes in shared state pending_request
    create oneshot channel, store tx in shared state
    wait on oneshot rx
    match rx result:
        Ok(response_bytes) => write response bytes back to client
        Err(Canceled) => client disconnected, just close
    clear pending_request and response_tx
}
```

Additional connections queue in the kernel's accept backlog.

### 4. actix-web HTTP server routes

| Route | Method | Handler |
|---|---|---|
| `/` | GET | Serve `bridge.html` via `include_str!("bridge.html")` with `Content-Type: text/html` |
| `/jsqr.js` | GET | Serve jsQR via `include_bytes!("jsqr.js")` with `Content-Type: application/javascript` |
| `/status` | GET | Return JSON: `{"pending": true, "size": 123}` or `{"pending": false}` |
| `/qr.svg` | GET | Generate SVG QR code from pending bytes. If no pending request, return 204 No Content. Content-Type: `image/svg+xml` |
| `/pending-bytes` | GET | Return JSON: `{"pending": true, "bytes": "<base64>"}` when request is pending, or `{"pending": false}` when nothing is pending. Used by automated test agents. |
| `/response` | POST | Accept `{"bytes": "base64..."}`, decode, send via oneshot. If no pending, return 400. |

Use `actix_web::web::Data<SharedState>` to inject the shared state into handlers.

For `/qr.svg`, use `actix_web::HttpResponse::Ok().content_type("image/svg+xml").body(svg_string)`.

For `/response`, extract JSON body with `actix_web::web::Json`, decode base64 with `base64::Engine` (use `base64::engine::general_purpose::STANDARD`).

### 5. QR code generation

```rust
use qrcode::{QrCode, EcLevel};
use qrcode::render::svg;

fn generate_qr_svg(bytes: &[u8]) -> String {
    let code = QrCode::with_error_correction_level(bytes, EcLevel::L).unwrap();
    code.render()
        .min_dimensions(400, 400)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build()
}
```

### 6. HTML page (`bridge.html`)

The page should be fully self-contained (all CSS inlined, no external resources). Dark theme, minimal and clean.

**Structure:**

```
<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Paypunk Bridge</title>
  <style>
    /* Dark theme: background #1a1a2e, cards #16213e, accent #0f3460, text #e0e0e0 */
    /* Centered layout, max-width 600px */
    /* QR display area: large, centered */
    /* Webcam feed: smaller, below QR, rounded corners */
    /* Status text + colored dot indicator */
    /* Error message styling */
  </style>
</head>
<body>
  <div id="app">
    <h1>Paypunk Bridge</h1>
    <div id="status">
      <span id="status-dot" class="dot grey"></span>
      <span id="status-text">Waiting for request...</span>
    </div>
    <div id="qr-area" style="display:none">
      <img id="qr-image" src="">
    </div>
    <div id="camera-area" style="display:none">
      <video id="video" autoplay playsinline></video>
      <canvas id="canvas" style="display:none"></canvas>
    </div>
    <div id="error" style="display:none"></div>
  </div>
  <script src="/jsqr.js"></script>
  <script>
    // Poll /status every 500ms
    // When pending becomes true:
    //   - Update QR image src to /qr.svg?t=<timestamp>
    //   - Show QR area
    //   - Start webcam (getUserMedia with {video: {facingMode: "environment"}})
    //   - On each animation frame, draw video to canvas, call jsQR(canvas data)
    //   - If QR decoded: stop webcam, POST bytes (base64) to /response
    //   - Update status text + dot color
    // When pending becomes false (after being true):
    //   - Hide QR and camera areas
    //   - Show "Waiting for request..."
    // Handle webcam errors: show message in #error div
    // Handle polling errors: retry after 1s
  </script>
</body>
</html>
```

**Key JS logic pattern for jsQR scanning:**

```javascript
const video = document.getElementById('video');
const canvas = document.getElementById('canvas');
const ctx = canvas.getContext('2d');

async function startCamera() {
    const stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: "environment" } });
    video.srcObject = stream;
    await video.play();
    canvas.width = video.videoWidth;
    canvas.height = video.videoHeight;
    scanFrame();
}

function scanFrame() {
    if (video.readyState !== video.HAVE_ENOUGH_DATA) {
        requestAnimationFrame(scanFrame);
        return;
    }
    ctx.drawImage(video, 0, 0);
    const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
    const code = jsQR(imageData.data, imageData.width, imageData.height);
    if (code) {
        // code.binaryData contains the decoded bytes
        sendResponse(code.binaryData);
        return;
    }
    requestAnimationFrame(scanFrame);
}

async function sendResponse(bytes) {
    // Stop webcam
    video.srcObject.getTracks().forEach(t => t.stop());
    // POST to /response
    const base64 = btoa(String.fromCharCode(...bytes));
    await fetch('/response', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ bytes: base64 })
    });
    // Update status
}
```

### 7. jsQR integration

- Download jsQR from `https://raw.githubusercontent.com/cozmo/jsQR/master/dist/jsQR.js` and save to `bridge/src/jsqr.js`
- Include via `<script src="/jsqr.js"></script>`
- The library exposes a global `jsQR(data, width, height)` function that returns `{ binaryData: number[], data: string }` or `null`

## Acceptance criteria

### Automated test (add to `bridge/src/lib.rs` or `bridge/tests/`)

Add a test module that:
- Creates a `BridgeConfig` with a temp socket path and a port
- Starts the bridge in a background task
- Connects to the Unix socket and sends test bytes
- Polls `/status` until pending is true
- Verifies `/qr.svg` returns SVG content
- POSTs a response to `/response`
- Verifies the Unix socket client received the response bytes
- Shuts down

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_bridge_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("test.sock").to_string_lossy().to_string();
        let port = 0; // OS assigns ephemeral port

        let config = BridgeConfig {
            port,
            socket_path: socket_path.clone(),
        };

        // Start bridge in background
        let handle = tokio::spawn(async move {
            run(config).await.unwrap();
        });

        // Give it time to start
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Connect Unix socket and send bytes
        let mut stream = UnixStream::connect(&socket_path).await.unwrap();
        let request_bytes = b"hello bridge";
        stream.write_all(request_bytes).await.unwrap();

        // Wait for response
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        assert_eq!(response, b"response bytes");

        handle.abort();
    }
}
```

Note: The actual test needs to know the assigned port to hit the HTTP server. If port is 0, the bridge needs to expose the actual port it bound to. Simplify by using a fixed port in tests (e.g., `0` for ephemeral with a mechanism to discover it, or hardcode a test port).

### Manual acceptance criteria

- [ ] `cargo build -p paypunk-bridge` succeeds
- [ ] `cargo test -p paypunk-bridge` passes
- [ ] Running `cargo run -p paypunk-bridge` prints startup message and blocks
- [ ] `curl http://localhost:12345/` returns the HTML page (200)
- [ ] `curl http://localhost:12345/status` returns `{"pending":false}`
- [ ] Sending bytes via Unix socket (`echo -n "test" | nc -U /tmp/keypunkd.sock` in one terminal) causes `/status` to return `{"pending":true,"size":4}`
- [ ] `curl http://localhost:12345/qr.svg` returns SVG when pending, 204 otherwise
- [ ] POSTing to `/response` with valid base64 bytes returns success and the Unix socket client receives those bytes
- [ ] POSTing to `/response` when no request is pending returns 400
- [ ] `curl http://localhost:12345/pending-bytes` returns `{"pending":false}` when idle
- [ ] While a request is pending, `curl http://localhost:12345/pending-bytes` returns `{"pending":true,"bytes":"<base64>"}` with the correct bytes
- [ ] Ctrl+C cleans up the socket file (`/tmp/keypunkd.sock` is removed)

## Context

- The bridge replaces keypunkd's socket — paypunkd connects to `/tmp/keypunkd.sock` transparently
- Messages are pure bytes, no IPC parsing needed
- QR code uses Low error correction (max ~2953 bytes); larger messages return an error to the Unix client
- The page stays alive after a response and waits for the next request (polling continues)
- Unix socket client disconnect is detected via oneshot `Canceled`; page resets on next poll
- `include_str!("bridge.html")` and `include_bytes!("jsqr.js")` resolve relative to the source file in `bridge/src/`
- Use `base64::engine::general_purpose::STANDARD` for base64 encode/decode
- No existing crate code outside `bridge/` is modified

## Implementation instructions for agent

1. Download jsQR from `https://raw.githubusercontent.com/cozmo/jsQR/master/dist/jsQR.js` and save to `bridge/src/jsqr.js`
2. Implement the full `bridge/src/lib.rs` with:
   - `BridgeConfig` and `BridgeState` types
   - Shared state with `Arc<Mutex<>>`
   - `run()` function with Unix socket listener + actix-web server
   - All HTTP route handlers
   - QR code generation function
   - Test module
3. Create the HTML page at `bridge/src/bridge.html` with the dark theme design and JS logic described above
4. Run `cargo build -p paypunk-bridge` and fix any compilation errors
5. Run `cargo test -p paypunk-bridge` and ensure tests pass
6. Manual test the full flow (see acceptance criteria)
7. Run `cargo fmt --all`
8. Move this step file to `project/done/02_step.md`
9. Commit with message: `feat: implement bridge core with Unix socket and QR web interface`
