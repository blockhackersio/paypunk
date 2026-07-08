# Step: Create the pong library crate

Create the `pong/` crate — a library that provides a handler struct for converting ping IPC messages to pong responses. Used by automated test agents to respond to bridge requests.

**This step is purely additive.** Only the root `Cargo.toml` (add workspace member) and the new `pong/` directory are touched.

## Tasks

### 1. Add `"pong"` to workspace members in root `Cargo.toml`

```toml
members = [
    ...
    "pong",
]
```

### 2. Create `pong/Cargo.toml`

```toml
[package]
name = "paypunk-pong"
version = "0.1.0"
edition = "2021"

[dependencies]
paypunk-ipc = { path = "../ipc" }
```

### 3. Create `pong/src/lib.rs`

A struct `PongHandler` with a single public method `handle()`.

**Function contract:**
- Input: A full application frame as stored by the bridge's pending bytes
  - Format: `[0x04 (MSG_APPLICATION)] [payload bytes] [32-byte MAC]`
- Output: A full response frame for the bridge to write back to the socket
  - Format: `[0x00 (success)] [response payload bytes]`
- Behavior:
  - Validate first byte is `MSG_APPLICATION` (0x04)
  - Strip the type byte (1 byte) and MAC tag (32 bytes) to extract the payload
  - If payload == `b"ping"`, return `[0x00, b'p', b'o', b'n', b'g']`
  - Otherwise return `Err` with descriptive message

```rust
use paypunk_ipc::messages::{MSG_APPLICATION, MAC_LEN};

pub struct PongHandler;

impl PongHandler {
    /// Handle a full application frame from the bridge.
    ///
    /// # Format
    /// - Input:  `[0x04] [payload] [32-byte MAC]`
    /// - Output: `[0x00] [response_payload]`
    pub fn handle(&self, frame: &[u8]) -> Result<Vec<u8>, String> {
        if frame.is_empty() || frame[0] != MSG_APPLICATION {
            return Err("expected MSG_APPLICATION frame".to_string());
        }
        if frame.len() < 1 + MAC_LEN {
            return Err(format!(
                "frame too short: {} bytes, need at least {}",
                frame.len(),
                1 + MAC_LEN
            ));
        }
        let payload = &frame[1..frame.len() - MAC_LEN];
        if payload == b"ping" {
            let mut response = vec![0x00u8]; // success status byte
            response.extend_from_slice(b"pong");
            Ok(response)
        } else {
            Err(format!(
                "expected ping payload, got: {:?}",
                String::from_utf8_lossy(payload)
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ping_returns_pong() {
        let handler = PongHandler;
        let mut frame = vec![MSG_APPLICATION];
        frame.extend_from_slice(b"ping");
        frame.extend_from_slice(&[0u8; MAC_LEN]); // dummy MAC

        let result = handler.handle(&frame).unwrap();
        assert_eq!(result, vec![0x00, b'p', b'o', b'n', b'g']);
    }

    #[test]
    fn test_invalid_message_type() {
        let handler = PongHandler;
        // Wrong type byte (not MSG_APPLICATION)
        let frame = vec![0xFF, b'p', b'i', b'n', b'g'];
        assert!(handler.handle(&frame).is_err());
    }

    #[test]
    fn test_frame_too_short() {
        let handler = PongHandler;
        // Only type byte, no payload or MAC
        let frame = vec![MSG_APPLICATION];
        assert!(handler.handle(&frame).is_err());
    }

    #[test]
    fn test_wrong_payload() {
        let handler = PongHandler;
        let mut frame = vec![MSG_APPLICATION];
        frame.extend_from_slice(b"notping");
        frame.extend_from_slice(&[0u8; MAC_LEN]);

        let result = handler.handle(&frame);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected ping"));
    }

    #[test]
    fn test_empty_frame() {
        let handler = PongHandler;
        assert!(handler.handle(&[]).is_err());
    }
}
```

## Acceptance criteria

- [ ] `cargo build` succeeds from workspace root
- [ ] `cargo build -p paypunk-pong` succeeds
- [ ] `cargo test -p paypunk-pong` passes (all 5 tests)

## Context

- The pong library is used by automated test agents that interact with the bridge's HTTP API
- A test agent reads pending bytes from `GET /pending-bytes`, feeds them to `PongHandler::handle()`, and POSTs the result to `/response`
- The bridge writes the response bytes back to the waiting Unix socket client
- Uses constants from `paypunk_ipc::messages` (`MSG_APPLICATION` = 0x04, `MAC_LEN` = 32) rather than hardcoding values
- The `[0x00]` status byte in the response follows the IPC receiver's response format (0 = success)
- No existing crate code outside `pong/` is modified

## Implementation instructions for agent

1. Add `"pong"` to workspace members in root `Cargo.toml`
2. Create `pong/Cargo.toml`
3. Create `pong/src/lib.rs` with `PongHandler` struct and tests
4. Run `cargo build` to verify it compiles
5. Run `cargo test -p paypunk-pong` to verify tests pass
6. Run `cargo fmt --all`
7. Move this step file to `project/done/03_step.md`
8. Commit with message: `feat: create pong library for ping-pong IPC testing`
