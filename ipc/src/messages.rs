use tactix::Message;

/// Universal IPC message — raw bytes over the wire.
/// The sender and receiver each handle their own serialization.
#[derive(Message)]
#[response(Result<Vec<u8>, String>)]
pub struct IpcMessage(pub Vec<u8>);
