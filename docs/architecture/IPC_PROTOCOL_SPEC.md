# AsterShell — Inter-Process Communication (IPC) Protocol Specification

This document details the binary wire protocol, authentication, message framing, version negotiation, and schema definitions implemented in the `ipc` crate.

---

## 1. Transport & Security Model

- **Transport:** Windows Local Named Pipes.
- **Pipe Address Format:** `\\.\pipe\astershell-ipc-{SHA256(UserName + SessionId)[0..16]}`
  - Example: `\\.\pipe\astershell-ipc-9a7f3c1b0e4d2a8f`
- **Security Descriptor (DACL):**
  - Configured with `SECURITY_ATTRIBUTES` using an SDDL string strictly limiting access to the current logged-in user:
    `D:(A;;GRGW;;;{UserTokenSID})(A;;GA;;;BA)`
  - Denies access across different user accounts, session boundaries, or unprivileged sandboxes.

---

## 2. Binary Framing & Header Specification

Every message transmitted over the named pipe is prefixed with an immutable 8-byte frame header:

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                       Magic: "ASTS" (0x53545341)              |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|         Version (u16)         |       Message Length (u16)    |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                         Payload Bytes ...                     |
+---------------------------------------------------------------+
```

### Fields:
- **`Magic` (`[u8; 4]`):** Must be exactly `b"ASTS"` (`0x41, 0x53, 0x54, 0x53`). Frames with invalid magic are rejected immediately and the connection is closed.
- **`Version` (`u16`):** Protocol version identifier. Current version is `1`.
- **`Message Length` (`u16`):** Length of payload in bytes (maximum frame payload size: 65,535 bytes). Larger payloads (e.g., image thumbnails) are chunked or passed via memory-mapped files.
- **`Payload` (`[u8]`):** Compact serialized binary data using `postcard`.

---

## 3. Handshake & Version Negotiation

When a client (`taskbar`, `search-ui`, etc.) connects to `shell-host`:

```
Client                                                  Server (shell-host)
  |                                                              |
  | -------- HandshakeRequest { client_id, client_version } ---> |
  |                                                              |
  | <------- HandshakeResponse { status: Ok, active_version } -- |
  |                                                              |
```

### Version Compatibility Rules:
1. If `client_version == server_version`: Connection proceeds with negotiated version.
2. If `client_version < server_version`: Server determines backward compatibility. If compatible, server responds with `client_version`.
3. If incompatible: Server responds with `HandshakeResponse::IncompatibleVersion { min_supported, max_supported }` and terminates pipe connection.

---

## 4. Message Schemas (Rust Types)

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcEnvelope {
    Handshake(HandshakePayload),
    Heartbeat { timestamp_ms: u64 },
    Window(WindowEventPayload),
    Search(SearchPayload),
    Launcher(LauncherPayload),
    Config(ConfigPayload),
    System(SystemPayload),
    Error(ErrorPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WindowEventPayload {
    Created { hwnd: u64, title: String, class_name: String, process_name: String },
    Destroyed { hwnd: u64 },
    Activated { hwnd: u64 },
    StateChanged { hwnd: u64, is_minimized: bool, is_maximized: bool },
    TitleChanged { hwnd: u64, new_title: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SearchPayload {
    ExecuteQuery { query_id: u32, query_string: String },
    CancelQuery { query_id: u32 },
    ResultBatch { query_id: u32, is_final: bool, items: Vec<SearchResultItem> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub category: String,
    pub icon_path: Option<String>,
    pub action_verb: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigPayload {
    Subscribe,
    UpdateBroadcast { config_json: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPayload {
    pub code: u32,
    pub message: String,
    pub source_component: String,
}
```

---

## 5. Configuration Distribution

1. The `configuration` crate maintains the authoritative configuration state in `shell-host` loaded from `~/.config/astershell/config.toml`.
2. When the configuration file is modified on disk, `shell-host` detects the modification using `ReadDirectoryChangesW`.
3. `shell-host` validates the updated schema. If valid, it broadcasts a `ConfigPayload::UpdateBroadcast` message across all active child pipes.
4. UI components apply hot-reloaded visual changes (colors, theme, dimensions) dynamically without restarting.
