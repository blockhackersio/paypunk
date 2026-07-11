# Bidirectional Animated-QR Transport: Build Guide

A guide for moving payloads larger than a single QR code can hold between the
**bridge** (a browser-based coordinator on a laptop, code in `./bridge`) and the
**signer** (an offline Tauri app on a phone, code in `./signer`), in **both**
directions, over the camera/screen air gap.

---

## 1. The problem and the shape of the solution

A single QR code tops out at **2,953 bytes** (version 40, error-correction level
L), and in practice less, because dense codes are hard for phone cameras to
read. Once your payloads exceed that, one static code can't carry them.

The fix is a **multipart animation**: split the payload into fragments, show one
QR per fragment, cycle through them, and let the receiver reassemble. Using
*fountain codes* on top means the receiver can start scanning at any frame and
tolerate dropped frames — it just keeps watching until it has enough parts.

### The key architectural insight

In this setup **both endpoints are web frontends**:

- **`bridge`** — a website in a normal desktop browser (`./bridge`). Its backend
  only reads and writes a Unix socket; it never touches QR.
- **`signer`** — a Tauri (Android) webview app (`./signer`), the offline signer.
  Same story: the native side stays dumb.

That means the entire QR problem lives in **JavaScript**, and it is
**symmetric**. Each side needs exactly two pieces:

| Component | Job |
|-----------|-----|
| **Encoder → display** | turn a byte payload into a cycling canvas animation |
| **Camera → decoder** | run a camera loop and reassemble frames into bytes |

The `bridge` page and the `signer` app run the **same two pieces**. The only
difference is what each does with the assembled bytes (write to the socket vs.
hand to the signer). So you write the transport once and reuse it on both ends.

```
        ┌────────────── bridge — ./bridge (browser) ─────────────┐
        │  backend: unix socket  ⇄  websocket  ⇄  frontend JS     │
        │                                     ├── encoder→canvas  │
        │                                     └── camera→decoder  │
        └────────────────────────────────────────────────────────┘
                     │  screen → camera        ▲  camera ← screen
                     ▼  (bridge shows QR)      │  (signer shows QR)
        ┌────────── signer — ./signer (Tauri webview) ───────────┐
        │  native: offline signer  ⇄  frontend JS                 │
        │                                     ├── encoder→canvas  │
        │                                     └── camera→decoder  │
        └────────────────────────────────────────────────────────┘
```

The only genuine asymmetry is **camera access**: trivial in a desktop browser,
fiddly in the Tauri Android webview (see §7).

---

## 2. The transport protocol

The transport is [`@ngraveio/bc-ur`](https://github.com/ngraveio/bc-ur) v1, a
pure-JS implementation of Blockchain Commons' Uniform Resources with a built-in
fountain encoder/decoder. It runs identically in a desktop browser and a Tauri
webview, and it gives you both halves from one dependency. It's the same family
of codes hardware wallets use to move multi-KB signing payloads across an air
gap, so the lossy-channel behavior is well tested. Pin it to `^1`; the v1 API
(`UR` / `UREncoder` / `URDecoder`) is what the code below uses.

---

## 3. The shared transport module

Define one small interface and implement it once. Everything else in this guide
depends only on this interface.

The three transport files in this section (`qr-transport.js`, `qr-display.js`,
`qr-scan.js`) are **identical on both ends**. Put a copy in each project:
`bridge/src/` and `signer/src/`.

`bridge/src/qr-transport.js`  ·  `signer/src/qr-transport.js`

```js
// Uniform interface used by BOTH the display and scan halves,
// in BOTH the bridge and the signer.
//
//   const enc = createEncoder(bytes);        // bytes: Uint8Array
//   const part = enc.nextPart();             // -> string (call repeatedly)
//
//   const dec = createDecoder();
//   dec.receive(partString);                 // feed each scanned frame
//   dec.progress;                            // 0..1
//   dec.isComplete;                          // boolean
//   dec.result;                              // Uint8Array (when complete)

import { UR, UREncoder, URDecoder } from '@ngraveio/bc-ur';

// Max CBOR bytes per fragment. Smaller fragments make lower-density QR codes
// that scan more reliably, at the cost of more frames.
const MAX_FRAGMENT_LEN = 200;

export function createEncoder(bytes) {
  const ur = UR.fromBuffer(Buffer.from(bytes));
  const encoder = new UREncoder(ur, MAX_FRAGMENT_LEN, 0);
  return {
    nextPart: () => encoder.nextPart(),        // "ur:bytes/…"
    get fragmentCount() { return encoder.fragmentsLength; },
  };
}

export function createDecoder() {
  const decoder = new URDecoder();
  return {
    receive(part) {
      try { decoder.receivePart(part); } catch { /* ignore junk frames */ }
    },
    get progress() { return decoder.estimatedPercentComplete(); }, // 0..1
    get isComplete() { return decoder.isComplete(); },
    get result() {
      if (!decoder.isComplete()) return null;
      const ur = decoder.resultUR();
      return new Uint8Array(ur.decodeCBOR()); // -> original bytes
    },
  };
}
```

> **Bundler note:** bc-ur v1 uses Node's `Buffer`. In Vite/webpack, add a
> `Buffer` polyfill: install `vite-plugin-node-polyfills` and enable it, or set
> `globalThis.Buffer = Buffer` from the `buffer` package in your entry file.

---

## 4. The display half (encoder → canvas)

Shared by both ends. Cycles the encoder's parts onto a `<canvas>` at a fixed
frame rate.

`bridge/src/qr-display.js`  ·  `signer/src/qr-display.js`

```js
import QRCode from 'qrcode';         // dependency: qrcode
import { createEncoder } from './qr-transport.js';

const FRAME_RATE = 8;                // frames per second (see §8)

// Renders `bytes` as an endless animated QR on `canvas`.
// Returns a stop() function.
export function startDisplay(canvas, bytes) {
  const encoder = createEncoder(bytes);
  let timer = null;

  async function tick() {
    const part = encoder.nextPart();
    await QRCode.toCanvas(canvas, part, {
      errorCorrectionLevel: 'L',   // lowest EC = most data per frame
      margin: 2,
      width: canvas.width,
    });
  }

  timer = setInterval(tick, Math.round(1000 / FRAME_RATE));
  tick();
  return () => clearInterval(timer);
}
```

Usage:

```js
const stop = startDisplay(document.getElementById('qr'), payloadBytes);
// call stop() once the peer signals it has fully received the payload
```

---

## 5. The scan half (camera → decoder)

Shared by both ends. Opens the camera, decodes each video frame with
[`jsQR`](https://github.com/cozmo/jsQR), and feeds strings to the decoder until
it completes.

`bridge/src/qr-scan.js`  ·  `signer/src/qr-scan.js`

```js
import jsQR from 'jsqr';            // dependency: jsqr
import { createDecoder } from './qr-transport.js';

// Scans an animated QR from the camera. Resolves with the reassembled bytes.
// onProgress(p) is called with 0..1 as fragments arrive.
export async function scanBytes(videoEl, { onProgress } = {}) {
  const stream = await navigator.mediaDevices.getUserMedia({
    video: { facingMode: 'environment' },
    audio: false,
  });
  videoEl.srcObject = stream;
  await videoEl.play();

  const canvas = document.createElement('canvas');
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  const decoder = createDecoder();

  return new Promise((resolve) => {
    let raf = 0;
    const stop = () => {
      cancelAnimationFrame(raf);
      stream.getTracks().forEach((t) => t.stop());
    };

    function loop() {
      if (videoEl.readyState === videoEl.HAVE_ENOUGH_DATA) {
        canvas.width = videoEl.videoWidth;
        canvas.height = videoEl.videoHeight;
        ctx.drawImage(videoEl, 0, 0, canvas.width, canvas.height);
        const img = ctx.getImageData(0, 0, canvas.width, canvas.height);
        const code = jsQR(img.data, img.width, img.height, {
          inversionAttempts: 'dontInvert',
        });
        if (code && code.data) {
          decoder.receive(code.data);
          onProgress?.(decoder.progress);
          if (decoder.isComplete) {
            const bytes = decoder.result;
            stop();
            resolve(bytes);
            return;
          }
        }
      }
      raf = requestAnimationFrame(loop);
    }
    loop();
  });
}
```

> **Why not the Tauri barcode-scanner plugin's `scan()` for reading?** It's
> one-shot: it opens the camera, decodes a single code, and closes. For an
> animated sequence you'd fight it re-opening per frame. The continuous
> `getUserMedia` loop above reads many frames from one camera session, which is
> what animated QR needs. You still *install* the plugin — but only for its
> permission plumbing (see §7).

---

## 6. The `bridge` website (`./bridge`)

### 6.1 Backend

The `bridge` backend does two things and nothing else:

1. Bridge the **Unix socket** to the browser over a **WebSocket**.
2. Serve the static frontend.

The wire contract:

- **socket → browser:** when bytes arrive on the Unix socket (a request to be
  signed), forward them to the browser as a binary WebSocket message. The
  browser displays them as an animated QR.
- **browser → socket:** when the browser finishes *scanning* the `signer`'s
  response, it sends those bytes back over the WebSocket; the backend writes them
  to the Unix socket.

This is deliberately dumb — no QR awareness. It's the same actix-web + tokio
shape as the original bridge, with one change: the old HTTP QR endpoints
(`/qr.svg`, `/pending-bytes`, `/response`) are replaced by a single WebSocket to
the browser. The existing x25519 + MAC handshake on the Unix socket stays exactly
as it was, so only the verified inner application payload is ever forwarded to the
browser.

Add the web-socket and static-file crates alongside your existing deps:

```toml
# bridge/Cargo.toml
actix-web  = "4"
actix-ws   = "0.3"
actix-files = "0.6"
futures-util = "0.3"
```

```rust
// bridge/src/bridge.rs — same structure as the original; QR moved to the frontend.
use std::sync::Arc;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Error};
use actix_files::Files;
use futures_util::StreamExt;
use tokio::sync::{oneshot, Mutex};
use paypunk_ipc::transport::UnixSocketTransport;
use paypunk_ipc::messages::{MAC_LEN, MSG_APPLICATION /* …plus the handshake consts */};

struct BridgeState {
    browser: Option<actix_ws::Session>,          // the connected QR frontend
    response_tx: Option<oneshot::Sender<Vec<u8>>>, // where the scanned reply goes
}
type SharedState = Arc<Mutex<BridgeState>>;

// The browser (QR display + scanner) connects here instead of polling /pending-bytes.
async fn ws(
    req: HttpRequest,
    body: web::Payload,
    state: web::Data<SharedState>,
) -> Result<HttpResponse, Error> {
    let (res, session, mut stream) = actix_ws::handle(&req, body)?;
    state.lock().await.browser = Some(session);

    let state = state.clone();
    actix_web::rt::spawn(async move {
        while let Some(Ok(actix_ws::Message::Binary(bytes))) = stream.next().await {
            // browser finished scanning the signer's response → hand it back to the socket
            if let Some(tx) = state.lock().await.response_tx.take() {
                let _ = tx.send(bytes.to_vec());
            }
        }
        state.lock().await.browser = None;
    });
    Ok(res)
}

pub async fn run(config: BridgeConfig) -> Result<(), Box<dyn std::error::Error>> {
    let _ = std::fs::remove_file(&config.socket_path);
    let state: SharedState = Arc::new(Mutex::new(BridgeState {
        browser: None,
        response_tx: None,
    }));
    let listener = tokio::net::UnixListener::bind(&config.socket_path)?;

    let http_state = state.clone();
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(http_state.clone()))
            .route("/ws", web::get().to(ws))
            .service(Files::new("/", "./dist").index_file("index.html")) // built frontend
    })
    .bind(format!("0.0.0.0:{}", config.port))?
    .run();

    let (secret, public) = generate_keypair(); // unchanged from the original
    let accept_loop = async {
        loop {
            if let Ok((stream, _)) = listener.accept().await {
                let st = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_ipc_connection(stream, st, secret, public).await {
                        eprintln!("bridge connection error: {e}");
                    }
                });
            }
        }
    };

    tokio::select! {
        _ = server => {},
        _ = accept_loop => {},
        _ = tokio::signal::ctrl_c() => { println!("\nshutting down..."); }
    }
    let _ = std::fs::remove_file(&config.socket_path);
    Ok(())
}

// The MSG_GET_PUBLIC_KEY / MSG_REGISTER_CLIENT branches are UNCHANGED from the
// original handle_ipc_connection; only MSG_APPLICATION now forwards over the ws.
async fn handle_ipc_connection(
    stream: tokio::net::UnixStream,
    state: SharedState,
    secret: [u8; 32],
    public: [u8; 32],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut transport = UnixSocketTransport::from_stream(stream);
    let mut hmac_key: Option<[u8; 32]> = None;

    loop {
        let frame = transport.read_frame().await?;
        if frame.is_empty() {
            return Ok(());
        }
        match frame[0] {
            // MSG_GET_PUBLIC_KEY  => { …reply with public key, unchanged… }
            // MSG_REGISTER_CLIENT => { …x25519, set hmac_key, ack, unchanged… }
            MSG_APPLICATION => {
                let payload = &frame[1..];
                let (msg, mac) = payload.split_at(payload.len() - MAC_LEN);
                let expected = compute_mac(hmac_key.as_ref().unwrap(), msg);
                if mac != expected {
                    return Err("MAC mismatch".into());
                }

                // forward the verified application payload to the browser → animated QR
                let (tx, rx) = oneshot::channel();
                {
                    let mut g = state.lock().await;
                    g.response_tx = Some(tx);
                    match g.browser.as_mut() {
                        Some(sess) => { let _ = sess.binary(msg.to_vec()).await; }
                        None => return Err("no browser connected".into()),
                    }
                }

                // wait for the browser to scan the signer's reply, then answer the socket
                let resp = rx.await.unwrap_or_else(|_| b"request cancelled".to_vec());
                transport.write_frame(&resp).await?;
            }
            other => {
                eprintln!("unknown IPC message type: {other}");
                return Ok(());
            }
        }
    }
}
```

Compared with the original, `BridgeState` swaps `pending_request` for a
`browser` WebSocket session; the `ws` handler replaces the old
`/pending-bytes` + `/response` pair; and `generate_qr_svg` / the `/qr.svg`
route are deleted outright, since the frontend now renders the animation.

### 6.2 Frontend (desktop browser)

Desktop `getUserMedia` needs no special setup beyond HTTPS or `localhost`. A
sketch tying both halves to the websocket:

```js
import { startDisplay } from './qr-display.js';
import { scanBytes } from './qr-scan.js';

const ws = new WebSocket(`ws://${location.host}/ws`);
ws.binaryType = 'arraybuffer';
const video = document.getElementById('camera');
const canvas = document.getElementById('qr');

let stopDisplay = null;

// A request arrived on the socket → show it as animated QR to the signer.
ws.onmessage = (ev) => {
  const requestBytes = new Uint8Array(ev.data);
  stopDisplay?.();
  stopDisplay = startDisplay(canvas, requestBytes);
};

// Operator taps "Scan response" after the signer signs and starts displaying.
document.getElementById('scan').onclick = async () => {
  const responseBytes = await scanBytes(video, {
    onProgress: (p) => (document.getElementById('pct').textContent =
      `${Math.round(p * 100)}%`),
  });
  stopDisplay?.();                 // stop showing the request
  ws.send(responseBytes);          // hand the signed response back to the socket
};
```

The `bridge` flow: **show request (display half) → operator moves phone →
scan response (scan half) → send to socket.**

---

## 7. The `signer` app (`./signer`, the part that fights you)

The `signer` runs the mirror image: **scan the request (camera) → hand to the
native signer → show the response (display).** The signing logic is native; the
QR transport is the same `qr-display.js` / `qr-scan.js` modules copied into
`signer/src/`.

### 7.1 Project setup

The `./signer` project already exists, so add the dependencies to it rather than
scaffolding a new app. From `./signer`:

```bash
cd signer
pnpm add @ngraveio/bc-ur@^1 qrcode jsqr
cargo tauri android init
```

Add the barcode-scanner plugin **for its camera-permission plumbing only** — you
won't call its `scan()` API:

```bash
cargo add tauri-plugin-barcode-scanner \
  --target 'cfg(target_os = "android")'
```

Initialize it in `signer/src-tauri/src/lib.rs`:

```rust
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .setup(|app| {
      #[cfg(mobile)]
      app.handle().plugin(tauri_plugin_barcode_scanner::init())?;
      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
```

### 7.2 Camera access — the critical fix

`getUserMedia` **does not work by default** in the Tauri Android webview (it works
fine in a desktop browser, which is why the `bridge` side needs none of this).
Android's WebView has a two-layer permission system, and Tauri doesn't wire up
the `WebChromeClient.onPermissionRequest` bridge out of the box, so the OS dialog
appearing does **not** mean the WebView receives the stream — both layers must be
satisfied independently.

Two things must both be true:

**1. Camera permission in the Android manifest.** The barcode-scanner plugin
(§7.1) adds this and, crucially, the `onPermissionRequest` bridge that makes
`getUserMedia` start working.

**2. CSP must allow the webview origin and blob media.** Tauri Android serves
from `https://tauri.localhost`; if that origin and `blob:` media aren't in your
CSP, Android silently blocks the camera. Set this in
`signer/src-tauri/tauri.conf.json`:

```json
{
  "app": {
    "security": {
      "csp": "default-src 'self' https://tauri.localhost; img-src 'self' data: blob: https://tauri.localhost; media-src 'self' data: blob:; connect-src 'self' ipc: http://ipc.localhost https://ipc.localhost https://tauri.localhost"
    }
  }
}
```

After these, the same `scanBytes()` from §5 runs unchanged in the webview: the
permission prompt fires automatically the first time `getUserMedia` is called.

### 7.3 Frontend (Tauri webview)

Identical modules, mirrored flow. The bytes handed to/from the signer travel over
Tauri's `invoke` instead of a websocket:

```js
import { invoke } from '@tauri-apps/api/core';
import { startDisplay } from './qr-display.js';
import { scanBytes } from './qr-scan.js';

const video = document.getElementById('camera');
const canvas = document.getElementById('qr');

document.getElementById('start').onclick = async () => {
  // 1. Scan the request the bridge is displaying.
  const requestBytes = await scanBytes(video, {
    onProgress: (p) => (document.getElementById('pct').textContent =
      `${Math.round(p * 100)}%`),
  });

  // 2. Hand it to the offline signer (native Rust command).
  const responseBytes = new Uint8Array(
    await invoke('sign_request', { request: Array.from(requestBytes) })
  );

  // 3. Display the signed response for the bridge's camera to read.
  startDisplay(canvas, responseBytes);
};
```

On the Rust side (in `./signer`), `sign_request` is an ordinary command that does
the signer's existing x25519 + MAC + signing work and returns the response
bytes — no QR, no sockets:

```rust
#[tauri::command]
fn sign_request(request: Vec<u8>) -> Vec<u8> {
    // verify MAC, sign, produce response frame …
    todo!()
}
```

---

## 8. Settings

These values are fixed in the code above and chosen to keep capacity high while
staying readable across the screen-to-camera gap:

- **Fragment size** (`MAX_FRAGMENT_LEN` in §3) is `200`. Smaller fragments make
  lower-density QR codes that scan more reliably, at the cost of more frames.
- **Frame rate** (`FRAME_RATE` in §4) is `8`. This is slow enough that the camera
  never catches a mid-transition frame and fast enough to finish quickly.
- **Error-correction level** (§4) is `'L'`, the minimum, for maximum payload per
  frame. The fountain layer already handles dropped frames, so error-correction
  headroom in each QR isn't needed.

**Compress before encoding.** Deflate/gzip the payload before handing bytes to
the encoder. This reduces the frame count directly and composes with the
transport.

---

## 9. Gotchas checklist

- [ ] **Tauri Android camera:** barcode-scanner plugin installed (permission +
      `onPermissionRequest` bridge) and CSP includes `tauri.localhost` + `blob:`.
      Without both, `getUserMedia` silently yields no stream.
- [ ] **Don't use the plugin's `scan()` for animated reads** — it's one-shot. Use
      the continuous `getUserMedia` loop.
- [ ] **bc-ur v1 needs a `Buffer` polyfill** in browser bundlers (§3). Pin to
      `^1` so the `UREncoder` / `URDecoder` API matches the code.
- [ ] **`willReadFrequently: true`** on the scan canvas context, or per-frame
      `getImageData` will be slow and janky.
- [ ] **Screen brightness / glare:** reading an LCD with a camera (either
      direction) is sensitive to glare and viewing angle; test in real lighting.
- [ ] **Stop the display** once the peer confirms receipt, so a stale animation
      isn't accidentally re-scanned.
- [ ] **`bridge` backend stays dumb:** no QR logic server-side. When migrating
      the existing `./bridge` code, delete `generate_qr_svg`, `/qr.svg`,
      `/pending-bytes`, etc.

---

## 10. Reference links

- BC-UR (JS, fountain encoder/decoder): <https://github.com/ngraveio/bc-ur>
- jsQR (browser QR decode): <https://github.com/cozmo/jsQR>
- `qrcode` (canvas QR render): <https://www.npmjs.com/package/qrcode>
- Tauri barcode-scanner plugin: <https://v2.tauri.app/plugin/barcode-scanner/>
- Tauri Android camera permission discussion:
  <https://github.com/orgs/tauri-apps/discussions/12732>
- Blockchain Commons animated QRs (background):
  <https://developer.blockchaincommons.com/animated-qrs/>
