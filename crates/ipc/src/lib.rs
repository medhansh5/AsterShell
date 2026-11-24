//! AsterShell Versioned Inter-Process Communication (IPC) Subsystem

use astercore::{AsterError, DiscoveredApp, Result};
use serde::{Deserialize, Serialize};

pub const FRAME_MAGIC: [u8; 4] = *b"ASTS";
pub const PROTOCOL_VERSION_1: u16 = 1;
pub const MAX_PAYLOAD_SIZE: usize = 65535;

/// Immutable 8-byte framing header prefixed on all named pipe frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    pub magic: [u8; 4],
    pub version: u16,
    pub payload_len: u16,
}

impl FrameHeader {
    pub fn new(version: u16, payload_len: u16) -> Self {
        Self {
            magic: FRAME_MAGIC,
            version,
            payload_len,
        }
    }

    pub fn encode(&self) -> [u8; 8] {
        let mut buf = [0u8; 8];
        buf[0..4].copy_from_slice(&self.magic);
        buf[4..6].copy_from_slice(&self.version.to_le_bytes());
        buf[6..8].copy_from_slice(&self.payload_len.to_le_bytes());
        buf
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 8 {
            return Err(AsterError::Ipc(
                "Frame header buffer underflow (< 8 bytes)".into(),
            ));
        }

        let magic = [bytes[0], bytes[1], bytes[2], bytes[3]];
        if magic != FRAME_MAGIC {
            return Err(AsterError::Ipc(format!(
                "Invalid frame magic: {:?}, expected {:?}",
                magic, FRAME_MAGIC
            )));
        }

        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != PROTOCOL_VERSION_1 {
            return Err(AsterError::ProtocolVersionMismatch {
                expected: PROTOCOL_VERSION_1,
                found: version,
            });
        }

        let payload_len = u16::from_le_bytes([bytes[6], bytes[7]]);
        Ok(Self {
            magic,
            version,
            payload_len,
        })
    }
}

/// Versioned request message types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IpcRequest {
    Handshake {
        client_version: u16,
        client_name: String,
    },
    Ping {
        timestamp_ms: u64,
    },
    GetConfig,
    DiscoverApps,
    Shutdown,
}

/// Versioned response message types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IpcResponse {
    HandshakeAck {
        negotiated_version: u16,
        server_pid: u32,
    },
    Pong {
        timestamp_ms: u64,
    },
    ConfigJson(String),
    DiscoveredApps(Vec<DiscoveredApp>),
    Ack,
    Error(String),
}

/// Encapsulated request frame with unique tracking ID.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RequestEnvelope {
    pub request_id: u64,
    pub request: IpcRequest,
}

/// Encapsulated response frame matched to request ID.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResponseEnvelope {
    pub request_id: u64,
    pub response: IpcResponse,
}

/// Serializes and frames a payload into wire bytes.
pub fn encode_frame<T: Serialize>(payload: &T) -> Result<Vec<u8>> {
    let serialized = postcard::to_allocvec(payload)
        .map_err(|e| AsterError::Serialization(format!("Postcard encode error: {}", e)))?;

    if serialized.len() > MAX_PAYLOAD_SIZE {
        return Err(AsterError::Ipc(format!(
            "Payload length {} exceeds MAX_PAYLOAD_SIZE {}",
            serialized.len(),
            MAX_PAYLOAD_SIZE
        )));
    }

    let header = FrameHeader::new(PROTOCOL_VERSION_1, serialized.len() as u16);
    let mut frame = Vec::with_capacity(8 + serialized.len());
    frame.extend_from_slice(&header.encode());
    frame.extend_from_slice(&serialized);
    Ok(frame)
}

/// Parses and decodes a wire frame into the deserialized payload.
pub fn decode_frame<'a, T: Deserialize<'a>>(frame_bytes: &'a [u8]) -> Result<(FrameHeader, T)> {
    if frame_bytes.len() < 8 {
        return Err(AsterError::Ipc(
            "Insufficient frame length (< 8 bytes)".into(),
        ));
    }

    let header = FrameHeader::decode(&frame_bytes[0..8])?;
    let expected_total = 8 + header.payload_len as usize;

    if frame_bytes.len() < expected_total {
        return Err(AsterError::Ipc(format!(
            "Incomplete frame: expected {} bytes, got {}",
            expected_total,
            frame_bytes.len()
        )));
    }

    let payload_bytes = &frame_bytes[8..expected_total];
    let decoded = postcard::from_bytes(payload_bytes)
        .map_err(|e| AsterError::Serialization(format!("Postcard decode error: {}", e)))?;

    Ok((header, decoded))
}

#[cfg(windows)]
pub mod transport {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    pub use tokio::net::windows::named_pipe::{
        ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
    };

    /// Creates a Named Pipe server instance with standard buffer parameters.
    pub fn create_pipe_server(pipe_name: &str, is_first: bool) -> Result<NamedPipeServer> {
        let mut options = ServerOptions::new();
        options.first_pipe_instance(is_first);
        options.max_instances(16);
        options.in_buffer_size(65536);
        options.out_buffer_size(65536);

        options.create(pipe_name).map_err(|e| {
            AsterError::Ipc(format!(
                "Failed to create pipe server '{}': {}",
                pipe_name, e
            ))
        })
    }

    /// Connects a client to the Named Pipe server.
    pub async fn connect_pipe_client(pipe_name: &str) -> Result<NamedPipeClient> {
        ClientOptions::new().open(pipe_name).map_err(|e| {
            AsterError::Ipc(format!("Failed to connect to pipe '{}': {}", pipe_name, e))
        })
    }

    /// Reads an exact versioned frame from an async reader.
    pub async fn read_framed_message<R: AsyncReadExt + Unpin, T: for<'de> Deserialize<'de>>(
        reader: &mut R,
    ) -> Result<T> {
        let mut header_buf = [0u8; 8];
        reader
            .read_exact(&mut header_buf)
            .await
            .map_err(|e| AsterError::Ipc(format!("Pipe read header failed: {}", e)))?;

        let header = FrameHeader::decode(&header_buf)?;
        let mut payload_buf = vec![0u8; header.payload_len as usize];
        reader
            .read_exact(&mut payload_buf)
            .await
            .map_err(|e| AsterError::Ipc(format!("Pipe read payload failed: {}", e)))?;

        let decoded: T = postcard::from_bytes(&payload_buf)
            .map_err(|e| AsterError::Serialization(format!("Decode payload error: {}", e)))?;

        Ok(decoded)
    }

    /// Writes an exact versioned frame to an async writer.
    pub async fn write_framed_message<W: AsyncWriteExt + Unpin, T: Serialize>(
        writer: &mut W,
        payload: &T,
    ) -> Result<()> {
        let frame_bytes = encode_frame(payload)?;
        writer
            .write_all(&frame_bytes)
            .await
            .map_err(|e| AsterError::Ipc(format!("Pipe write frame failed: {}", e)))?;
        writer
            .flush()
            .await
            .map_err(|e| AsterError::Ipc(format!("Pipe flush failed: {}", e)))?;
        Ok(())
    }
}

pub fn crate_name() -> &'static str {
    "ipc"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_header_encode_decode() {
        let header = FrameHeader::new(1, 1024);
        let encoded = header.encode();
        let decoded = FrameHeader::decode(&encoded).expect("Decode should succeed");
        assert_eq!(decoded.magic, FRAME_MAGIC);
        assert_eq!(decoded.version, 1);
        assert_eq!(decoded.payload_len, 1024);
    }

    #[test]
    fn test_invalid_magic_rejected() {
        let mut bytes = [0u8; 8];
        bytes[0..4].copy_from_slice(b"NOPE");
        bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
        let res = FrameHeader::decode(&bytes);
        assert!(res.is_err());
    }

    #[test]
    fn test_version_mismatch_rejected() {
        let mut bytes = [0u8; 8];
        bytes[0..4].copy_from_slice(&FRAME_MAGIC);
        bytes[4..6].copy_from_slice(&2u16.to_le_bytes()); // Version 2
        let res = FrameHeader::decode(&bytes);
        assert!(matches!(
            res,
            Err(AsterError::ProtocolVersionMismatch {
                expected: 1,
                found: 2
            })
        ));
    }

    #[test]
    fn test_request_response_roundtrip() {
        let req = RequestEnvelope {
            request_id: 42,
            request: IpcRequest::Handshake {
                client_version: 1,
                client_name: "test-client".into(),
            },
        };

        let encoded = encode_frame(&req).expect("Encode failed");
        let (_header, decoded): (FrameHeader, RequestEnvelope) =
            decode_frame(&encoded).expect("Decode failed");

        assert_eq!(decoded.request_id, 42);
        assert_eq!(
            decoded.request,
            IpcRequest::Handshake {
                client_version: 1,
                client_name: "test-client".into()
            }
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_named_pipe_async_communication() {
        let pipe_name = r"\\.\pipe\astershell-test-pipe-1";
        let server = transport::create_pipe_server(pipe_name, true)
            .expect("Failed to create test pipe server");

        let server_task = tokio::spawn(async move {
            server
                .connect()
                .await
                .expect("Server failed to accept connection");
            let mut server = server;
            let req: RequestEnvelope = transport::read_framed_message(&mut server)
                .await
                .expect("Server failed to read message");

            assert_eq!(req.request_id, 101);
            let resp = ResponseEnvelope {
                request_id: req.request_id,
                response: IpcResponse::Pong {
                    timestamp_ms: 12345,
                },
            };
            transport::write_framed_message(&mut server, &resp)
                .await
                .expect("Server failed to write response");
        });

        let mut client = transport::connect_pipe_client(pipe_name)
            .await
            .expect("Client failed to connect");

        let req = RequestEnvelope {
            request_id: 101,
            request: IpcRequest::Ping {
                timestamp_ms: 12300,
            },
        };
        transport::write_framed_message(&mut client, &req)
            .await
            .expect("Client failed to send ping");

        let resp: ResponseEnvelope = transport::read_framed_message(&mut client)
            .await
            .expect("Client failed to read pong");

        assert_eq!(resp.request_id, 101);
        assert_eq!(
            resp.response,
            IpcResponse::Pong {
                timestamp_ms: 12345
            }
        );

        server_task.await.unwrap();
    }
}
