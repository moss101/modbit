//! JSON-RPC 2.0 framing and the MCP handshake (docs/16 "MCP / external
//! tools": dynamic `list → call → cancel`).
//!
//! The stdio transport is newline-delimited JSON: one message per line, no
//! embedded newline. Every decode is bounded — a server cannot make the
//! host buffer an unbounded line — and every message is validated as
//! JSON-RPC before anything reads its payload.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The MCP revision this client speaks.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// Revisions the client accepts a server answering with (a server may pick
/// an older one it supports; anything else ends the handshake).
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// Client name in the handshake.
pub const CLIENT_NAME: &str = "modbit";

/// `initialize`.
pub const METHOD_INITIALIZE: &str = "initialize";
/// `notifications/initialized`.
pub const METHOD_INITIALIZED: &str = "notifications/initialized";
/// `tools/list`.
pub const METHOD_TOOLS_LIST: &str = "tools/list";
/// `tools/call`.
pub const METHOD_TOOLS_CALL: &str = "tools/call";
/// `notifications/cancelled`.
pub const METHOD_CANCELLED: &str = "notifications/cancelled";
/// `ping`.
pub const METHOD_PING: &str = "ping";

/// A JSON-RPC error object.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcError {
    /// Code.
    pub code: i64,
    /// Message (server text: untrusted, bounded by the caller).
    pub message: String,
    /// Optional data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// One JSON-RPC message in either direction. A request has `id` and
/// `method`, a response has `id` and one of `result`/`error`, a
/// notification has `method` and no `id`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// Request/response correlation id (absent on a notification).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    /// Method (absent on a response).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// Parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    /// Result payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Error payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

/// What a decoded frame is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    /// `id` + `method`.
    Request,
    /// `id` + `result` or `error`.
    Response,
    /// `method`, no `id`.
    Notification,
}

/// A protocol-level failure: the peer is not speaking MCP correctly, or is
/// speaking it in a way the host will not accept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolError {
    /// Stable code (`FRAME_TOO_LARGE`, `FRAME_MALFORMED`, …).
    pub code: &'static str,
    /// What was wrong.
    pub message: String,
}

impl ProtocolError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl Frame {
    /// A request.
    #[must_use]
    pub fn request(id: u64, method: &str, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: Some(id),
            method: Some(method.into()),
            params: Some(params),
            ..Default::default()
        }
    }

    /// A notification (no id, never answered).
    #[must_use]
    pub fn notification(method: &str, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            method: Some(method.into()),
            params: Some(params),
            ..Default::default()
        }
    }

    /// A successful response (servers send these; the host builds them only
    /// to answer a server-initiated request it does not serve).
    #[must_use]
    pub fn response(id: u64, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: Some(id),
            result: Some(result),
            ..Default::default()
        }
    }

    /// An error response.
    #[must_use]
    pub fn error(id: u64, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: Some(id),
            error: Some(RpcError {
                code,
                message: message.into(),
                data: None,
            }),
            ..Default::default()
        }
    }

    /// What this frame is.
    #[must_use]
    pub fn kind(&self) -> FrameKind {
        match (self.id, self.method.is_some()) {
            (Some(_), true) => FrameKind::Request,
            (Some(_), false) => FrameKind::Response,
            (None, _) => FrameKind::Notification,
        }
    }

    /// One wire line: the JSON followed by `\n`. Serialization never emits
    /// a raw newline, so the framing cannot be broken by payload text.
    ///
    /// # Panics
    /// Never: a `Frame` is always serializable.
    #[must_use]
    pub fn encode(&self) -> String {
        let mut s = serde_json::to_string(self).expect("frame serializes");
        s.push('\n');
        s
    }
}

/// Decode one wire line, bounded by `max_bytes`.
///
/// # Errors
/// `FRAME_TOO_LARGE` when the line exceeds the bound, `FRAME_MALFORMED`
/// when it is not JSON or not an object, `JSONRPC_VERSION` when it does not
/// announce `"2.0"`, `FRAME_INCOMPLETE` when it is neither request,
/// response nor notification.
pub fn decode(line: &str, max_bytes: usize) -> Result<Frame, ProtocolError> {
    if line.len() > max_bytes {
        return Err(ProtocolError::new(
            "FRAME_TOO_LARGE",
            format!(
                "{} bytes exceeds the {max_bytes} byte frame bound",
                line.len()
            ),
        ));
    }
    let value: Value = serde_json::from_str(line)
        .map_err(|e| ProtocolError::new("FRAME_MALFORMED", format!("not JSON: {e}")))?;
    if !value.is_object() {
        return Err(ProtocolError::new(
            "FRAME_MALFORMED",
            "a JSON-RPC message is an object",
        ));
    }
    if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(ProtocolError::new(
            "JSONRPC_VERSION",
            "message does not announce jsonrpc 2.0",
        ));
    }
    // An id we cannot correlate is an id we will not answer: MCP allows a
    // string id, but this client only ever issues integers, so a response
    // carrying anything else belongs to nobody.
    if let Some(id) = value.get("id")
        && !id.is_null()
        && !id.is_u64()
    {
        return Err(ProtocolError::new(
            "FRAME_MALFORMED",
            "response id is not one this client issued",
        ));
    }
    let frame: Frame = serde_json::from_value(value)
        .map_err(|e| ProtocolError::new("FRAME_MALFORMED", format!("not a JSON-RPC frame: {e}")))?;
    if frame.id.is_none() && frame.method.is_none() {
        return Err(ProtocolError::new(
            "FRAME_INCOMPLETE",
            "neither request, response nor notification",
        ));
    }
    if frame.kind() == FrameKind::Response && frame.result.is_none() && frame.error.is_none() {
        return Err(ProtocolError::new(
            "FRAME_INCOMPLETE",
            "response carries neither result nor error",
        ));
    }
    Ok(frame)
}

/// The `initialize` request this client sends. It declares no client
/// capabilities: Modbit serves no sampling, roots or elicitation to a
/// server, so a server may not ask the host for anything.
#[must_use]
pub fn initialize_request(id: u64) -> Frame {
    Frame::request(
        id,
        METHOD_INITIALIZE,
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": CLIENT_NAME, "version": env!("CARGO_PKG_VERSION") },
        }),
    )
}

/// What the handshake established about a server. Both strings are the
/// server's own text: display only, bounded, never authority.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerInfo {
    /// Name the server calls itself.
    pub name: String,
    /// Version the server reports.
    pub version: String,
    /// Protocol revision agreed.
    pub protocol_version: String,
    /// Whether the server declared the `tools` capability.
    pub tools: bool,
    /// Whether the server said its tool list can change under it.
    pub tools_list_changed: bool,
}

/// Validate an `initialize` result and read what it establishes.
///
/// # Errors
/// `HANDSHAKE_MALFORMED` when the result is not an object,
/// `PROTOCOL_VERSION_UNSUPPORTED` when the server picked a revision this
/// client does not speak, `SERVER_HAS_NO_TOOLS` when it declares no `tools`
/// capability (there is then nothing for the hub to expose).
pub fn accept_initialize_result(
    result: &Value,
    max_text_bytes: usize,
) -> Result<ServerInfo, ProtocolError> {
    let obj = result.as_object().ok_or_else(|| {
        ProtocolError::new("HANDSHAKE_MALFORMED", "initialize result is not an object")
    })?;
    let version = obj
        .get("protocolVersion")
        .and_then(Value::as_str)
        .ok_or_else(|| ProtocolError::new("HANDSHAKE_MALFORMED", "no protocolVersion"))?;
    if !SUPPORTED_PROTOCOL_VERSIONS.contains(&version) {
        return Err(ProtocolError::new(
            "PROTOCOL_VERSION_UNSUPPORTED",
            format!(
                "server speaks `{version}`; this client speaks {SUPPORTED_PROTOCOL_VERSIONS:?}"
            ),
        ));
    }
    let caps = obj.get("capabilities");
    let tools_cap = caps.and_then(|c| c.get("tools"));
    if tools_cap.is_none() {
        return Err(ProtocolError::new(
            "SERVER_HAS_NO_TOOLS",
            "server declares no `tools` capability",
        ));
    }
    let info = obj.get("serverInfo");
    Ok(ServerInfo {
        name: bounded_text(info.and_then(|i| i.get("name")), max_text_bytes),
        version: bounded_text(info.and_then(|i| i.get("version")), max_text_bytes),
        protocol_version: version.to_owned(),
        tools: true,
        tools_list_changed: tools_cap
            .and_then(|t| t.get("listChanged"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// The `notifications/cancelled` a host sends for an in-flight request.
#[must_use]
pub fn cancelled_notification(request_id: u64, reason: &str) -> Frame {
    Frame::notification(
        METHOD_CANCELLED,
        json!({ "requestId": request_id, "reason": reason }),
    )
}

/// Server text as the host will keep it: control characters removed (they
/// break a terminal projection and hide content), trimmed to `max_bytes` on
/// a character boundary.
#[must_use]
pub fn bounded_text(value: Option<&Value>, max_bytes: usize) -> String {
    let raw = value.and_then(Value::as_str).unwrap_or_default();
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c == '\n' || c == '\t' {
                c
            } else if c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();
    truncate_on_boundary(&cleaned, max_bytes).0
}

/// Trim `s` to at most `max_bytes` without splitting a character. Returns
/// the text and whether anything was dropped.
#[must_use]
pub fn truncate_on_boundary(s: &str, max_bytes: usize) -> (String, bool) {
    if s.len() <= max_bytes {
        return (s.to_owned(), false);
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    (s[..end].to_owned(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_round_trips_as_one_line() {
        let f = Frame::request(7, METHOD_TOOLS_LIST, json!({}));
        let line = f.encode();
        assert!(line.ends_with('\n'));
        assert_eq!(line.matches('\n').count(), 1, "one message per line");
        let back = decode(line.trim_end(), 1 << 20).expect("decodes");
        assert_eq!(back, f);
        assert_eq!(back.kind(), FrameKind::Request);
    }

    #[test]
    fn payload_newlines_never_break_the_framing() {
        let f = Frame::request(1, METHOD_TOOLS_CALL, json!({ "x": "a\nb\nc" }));
        let line = f.encode();
        assert_eq!(line.matches('\n').count(), 1);
        let back = decode(line.trim_end(), 1 << 20).expect("decodes");
        assert_eq!(back.params.unwrap()["x"], json!("a\nb\nc"));
    }

    #[test]
    fn oversize_malformed_and_unversioned_lines_are_refused() {
        let big = Frame::request(1, METHOD_PING, json!({ "pad": "x".repeat(500) })).encode();
        assert_eq!(
            decode(big.trim_end(), 64).unwrap_err().code,
            "FRAME_TOO_LARGE"
        );
        assert_eq!(
            decode("not json", 1 << 20).unwrap_err().code,
            "FRAME_MALFORMED"
        );
        assert_eq!(
            decode("[1,2]", 1 << 20).unwrap_err().code,
            "FRAME_MALFORMED"
        );
        assert_eq!(
            decode(r#"{"id":1,"result":{}}"#, 1 << 20).unwrap_err().code,
            "JSONRPC_VERSION"
        );
        assert_eq!(
            decode(r#"{"jsonrpc":"2.0","id":1}"#, 1 << 20)
                .unwrap_err()
                .code,
            "FRAME_INCOMPLETE"
        );
        assert_eq!(
            decode(r#"{"jsonrpc":"2.0","id":"abc","result":{}}"#, 1 << 20)
                .unwrap_err()
                .code,
            "FRAME_MALFORMED",
            "an id this client never issued belongs to nobody"
        );
    }

    #[test]
    fn a_notification_has_no_id() {
        let n = cancelled_notification(3, "user cancelled");
        assert_eq!(n.kind(), FrameKind::Notification);
        assert_eq!(n.params.unwrap()["requestId"], json!(3));
    }

    #[test]
    fn the_handshake_accepts_a_supported_revision_and_refuses_the_rest() {
        let ok = json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": { "listChanged": true } },
            "serverInfo": { "name": "docs", "version": "1.2.3" },
        });
        let info = accept_initialize_result(&ok, 256).expect("accepted");
        assert_eq!(info.name, "docs");
        assert!(info.tools && info.tools_list_changed);

        let future = json!({ "protocolVersion": "2099-01-01", "capabilities": { "tools": {} } });
        assert_eq!(
            accept_initialize_result(&future, 256).unwrap_err().code,
            "PROTOCOL_VERSION_UNSUPPORTED"
        );
        let toolless = json!({ "protocolVersion": PROTOCOL_VERSION, "capabilities": {} });
        assert_eq!(
            accept_initialize_result(&toolless, 256).unwrap_err().code,
            "SERVER_HAS_NO_TOOLS"
        );
    }

    #[test]
    fn server_text_is_stripped_of_control_characters_and_bounded() {
        let v = json!("na\u{0007}me\u{001b}[31m");
        assert_eq!(bounded_text(Some(&v), 64), "na me [31m");
        let long = json!("é".repeat(100));
        let out = bounded_text(Some(&long), 9);
        assert!(out.len() <= 9 && out.chars().all(|c| c == 'é'));
    }
}
