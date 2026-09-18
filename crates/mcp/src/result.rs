//! What a `tools/call` returned, normalized into typed parts under the
//! host's bounds (docs/16 "MCP / external tools"; the media half of
//! REQ-EV-0187 consumes these parts).
//!
//! An MCP result is a list of content blocks the server chose. The host
//! keeps text, image, audio and resource blocks, each within a byte bound,
//! and drops the rest by kind. Binary payloads are validated as base64
//! before anything downstream trusts their length — a block claiming to be
//! an image but carrying arbitrary text never reaches the media pipeline.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::discovery::Limits;
use crate::protocol::{ProtocolError, bounded_text, truncate_on_boundary};

/// One content block the host kept.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Part {
    /// Text. Untrusted: it is data the model reads, never an instruction.
    Text {
        /// The text, control-stripped and bounded.
        text: String,
        /// Whether the bound cut it.
        truncated: bool,
    },
    /// An image the server returned.
    Image {
        /// Declared media type.
        mime_type: String,
        /// Raw bytes, decoded from the block's base64.
        #[serde(skip)]
        bytes: Vec<u8>,
        /// Decoded byte length (kept when the bytes are not serialized).
        byte_len: usize,
    },
    /// Audio the server returned.
    Audio {
        /// Declared media type.
        mime_type: String,
        /// Raw bytes.
        #[serde(skip)]
        bytes: Vec<u8>,
        /// Decoded byte length.
        byte_len: usize,
    },
    /// A resource the server embedded or referenced.
    Resource {
        /// Its URI as the server gave it (untrusted; the host never
        /// dereferences it on the server's say-so).
        uri: String,
        /// Declared media type, when any.
        mime_type: String,
        /// Embedded text, when the server embedded text.
        text: Option<String>,
        /// Embedded binary length, when the server embedded bytes.
        byte_len: Option<usize>,
    },
}

impl Part {
    /// How many bytes this part costs the host.
    #[must_use]
    pub fn byte_cost(&self) -> usize {
        match self {
            Part::Text { text, .. } => text.len(),
            Part::Image { byte_len, .. } | Part::Audio { byte_len, .. } => *byte_len,
            Part::Resource { text, byte_len, .. } => {
                text.as_ref().map_or(0, String::len) + byte_len.unwrap_or(0)
            }
        }
    }

    /// The part's kind as a stable word (audit and projection).
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Part::Text { .. } => "text",
            Part::Image { .. } => "image",
            Part::Audio { .. } => "audio",
            Part::Resource { .. } => "resource",
        }
    }
}

/// A block the host did not keep.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DroppedPart {
    /// Kind as the server named it.
    pub kind: String,
    /// Stable code.
    pub code: String,
    /// Why.
    pub reason: String,
}

/// A normalized `tools/call` result.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallResult {
    /// The blocks the host kept, in the order the server sent them.
    pub parts: Vec<Part>,
    /// Blocks the host refused.
    pub dropped: Vec<DroppedPart>,
    /// Whether the server reported the call itself failed (`isError`). This
    /// is an application failure of the external tool, not a transport one.
    pub is_error: bool,
    /// The server's structured output, when it returned one and it fits.
    pub structured: Option<Value>,
    /// How many times a secret in the host's custody was replaced in this
    /// answer (REQ-EV-0128: a credential is handed to a server to use, not
    /// to repeat — an answer carrying one back would put it in the model's
    /// context). 0 in the common case.
    #[serde(default)]
    pub redacted: usize,
}

impl CallResult {
    /// All text the result carries, joined — what a text-only projection
    /// shows and what the inline budget is measured against.
    #[must_use]
    pub fn text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                Part::Text { text, .. } => Some(text.as_str()),
                Part::Resource {
                    text: Some(text), ..
                } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Whether any part is media (an image or audio block) — what tells the
    /// host this result must go through the media pipeline.
    #[must_use]
    pub fn has_media(&self) -> bool {
        self.parts
            .iter()
            .any(|p| matches!(p, Part::Image { .. } | Part::Audio { .. }))
    }
}

fn drop_block(kind: &str, code: &str, reason: impl Into<String>) -> DroppedPart {
    DroppedPart {
        kind: kind.to_owned(),
        code: code.to_owned(),
        reason: reason.into(),
    }
}

/// Parse a `tools/call` result under `limits`.
///
/// # Errors
/// `CALL_RESULT_MALFORMED` when the result is not an object, or carries
/// neither a `content` array nor `structuredContent`.
pub fn parse_call_result(result: &Value, limits: &Limits) -> Result<CallResult, ProtocolError> {
    let obj = result.as_object().ok_or_else(|| ProtocolError {
        code: "CALL_RESULT_MALFORMED",
        message: "tools/call result is not an object".into(),
    })?;
    let content = obj.get("content").and_then(Value::as_array);
    let structured = obj.get("structuredContent");
    if content.is_none() && structured.is_none() {
        return Err(ProtocolError {
            code: "CALL_RESULT_MALFORMED",
            message: "tools/call result carries neither `content` nor `structuredContent`".into(),
        });
    }
    let mut out = CallResult {
        is_error: obj.get("isError").and_then(Value::as_bool).unwrap_or(false),
        structured: structured.and_then(|s| {
            let bytes = serde_json::to_string(s)
                .map(|t| t.len())
                .unwrap_or(usize::MAX);
            (bytes <= limits.max_text_part_bytes).then(|| s.clone())
        }),
        ..CallResult::default()
    };
    if structured.is_some() && out.structured.is_none() {
        out.dropped.push(drop_block(
            "structured",
            "PART_TOO_LARGE",
            format!(
                "structuredContent exceeds {} bytes",
                limits.max_text_part_bytes
            ),
        ));
    }
    let mut total = out.structured.as_ref().map_or(0, |s| s.to_string().len());
    for block in content.unwrap_or(&Vec::new()) {
        if out.parts.len() >= limits.max_result_parts {
            out.dropped.push(drop_block(
                "*",
                "TOO_MANY_PARTS",
                format!("result carries more than {} parts", limits.max_result_parts),
            ));
            break;
        }
        match parse_part(block, limits) {
            Ok(part) => {
                total += part.byte_cost();
                if total > limits.max_result_bytes {
                    out.dropped.push(drop_block(
                        part.kind(),
                        "RESULT_TOO_LARGE",
                        format!("result exceeds {} bytes in total", limits.max_result_bytes),
                    ));
                    break;
                }
                out.parts.push(part);
            }
            Err(d) => out.dropped.push(d),
        }
    }
    Ok(out)
}

fn parse_part(block: &Value, limits: &Limits) -> Result<Part, DroppedPart> {
    let obj = block.as_object().ok_or_else(|| {
        drop_block(
            "<unknown>",
            "PART_MALFORMED",
            "content block is not an object",
        )
    })?;
    let kind = obj
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("<untyped>");
    match kind {
        "text" => {
            let raw = bounded_text(obj.get("text"), limits.max_text_part_bytes * 2);
            let (text, truncated) = truncate_on_boundary(&raw, limits.max_text_part_bytes);
            Ok(Part::Text { text, truncated })
        }
        "image" | "audio" => {
            let mime = bounded_text(obj.get("mimeType"), 128);
            if mime.is_empty() {
                return Err(drop_block(kind, "PART_MALFORMED", "no mimeType"));
            }
            let bytes = decode_binary(kind, obj.get("data"), limits)?;
            let byte_len = bytes.len();
            Ok(if kind == "image" {
                Part::Image {
                    mime_type: mime,
                    bytes,
                    byte_len,
                }
            } else {
                Part::Audio {
                    mime_type: mime,
                    bytes,
                    byte_len,
                }
            })
        }
        "resource" | "resource_link" => {
            let res = obj.get("resource").unwrap_or(block);
            let uri = bounded_text(res.get("uri"), 2048);
            if uri.is_empty() {
                return Err(drop_block(kind, "PART_MALFORMED", "resource has no uri"));
            }
            let text = res.get("text").map(|_| {
                truncate_on_boundary(
                    &bounded_text(res.get("text"), limits.max_text_part_bytes * 2),
                    limits.max_text_part_bytes,
                )
                .0
            });
            let byte_len = match res.get("blob") {
                Some(b) => Some(decode_binary(kind, Some(b), limits)?.len()),
                None => None,
            };
            Ok(Part::Resource {
                uri,
                mime_type: bounded_text(res.get("mimeType"), 128),
                text,
                byte_len,
            })
        }
        other => Err(drop_block(
            other,
            "PART_KIND_UNKNOWN",
            "the host keeps text, image, audio and resource blocks",
        )),
    }
}

fn decode_binary(
    kind: &str,
    data: Option<&Value>,
    limits: &Limits,
) -> Result<Vec<u8>, DroppedPart> {
    let b64 = data
        .and_then(Value::as_str)
        .ok_or_else(|| drop_block(kind, "PART_MALFORMED", "no base64 payload"))?;
    // Reject on the encoded length first: decoding is what would cost the
    // memory, so the bound is checked before any of it is spent.
    if b64.len() / 4 * 3 > limits.max_binary_part_bytes {
        return Err(drop_block(
            kind,
            "PART_TOO_LARGE",
            format!("payload exceeds {} bytes", limits.max_binary_part_bytes),
        ));
    }
    let bytes = BASE64.decode(b64).map_err(|e| {
        drop_block(
            kind,
            "PART_NOT_BASE64",
            format!("payload is not base64: {e}"),
        )
    })?;
    if bytes.len() > limits.max_binary_part_bytes {
        return Err(drop_block(
            kind,
            "PART_TOO_LARGE",
            format!("payload exceeds {} bytes", limits.max_binary_part_bytes),
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn png() -> String {
        BASE64.encode([0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a])
    }

    #[test]
    fn text_and_image_blocks_are_kept_as_typed_parts() {
        let r = parse_call_result(
            &json!({ "content": [
                { "type": "text", "text": "the chart" },
                { "type": "image", "mimeType": "image/png", "data": png() },
            ]}),
            &Limits::default(),
        )
        .expect("parsed");
        assert_eq!(r.parts.len(), 2);
        assert_eq!(r.text(), "the chart");
        assert!(r.has_media());
        match &r.parts[1] {
            Part::Image {
                mime_type,
                bytes,
                byte_len,
            } => {
                assert_eq!(mime_type, "image/png");
                assert_eq!(*byte_len, 8);
                assert_eq!(&bytes[..4], b"\x89PNG");
            }
            other => panic!("expected an image, got {other:?}"),
        }
        assert!(!r.is_error);
    }

    #[test]
    fn a_tool_error_is_an_application_failure_the_host_reports() {
        let r = parse_call_result(
            &json!({ "content": [{ "type": "text", "text": "no such document" }], "isError": true }),
            &Limits::default(),
        )
        .expect("parsed");
        assert!(r.is_error);
        assert_eq!(r.text(), "no such document");
    }

    #[test]
    fn a_payload_that_is_not_base64_never_becomes_media() {
        let r = parse_call_result(
            &json!({ "content": [{ "type": "image", "mimeType": "image/png", "data": "<<<not base64>>>" }]}),
            &Limits::default(),
        )
        .expect("parsed");
        assert!(r.parts.is_empty());
        assert_eq!(r.dropped[0].code, "PART_NOT_BASE64");
        assert!(!r.has_media());
    }

    #[test]
    fn parts_and_bytes_are_bounded() {
        let limits = Limits {
            max_result_parts: 2,
            max_text_part_bytes: 8,
            max_binary_part_bytes: 4,
            ..Limits::default()
        };

        let r = parse_call_result(
            &json!({ "content": [
                { "type": "text", "text": "x".repeat(100) },
                { "type": "text", "text": "y" },
                { "type": "text", "text": "z" },
            ]}),
            &limits,
        )
        .expect("parsed");
        assert_eq!(r.parts.len(), 2);
        assert!(
            matches!(&r.parts[0], Part::Text { text, truncated } if text.len() == 8 && *truncated)
        );
        assert_eq!(r.dropped[0].code, "TOO_MANY_PARTS");

        let r = parse_call_result(
            &json!({ "content": [{ "type": "image", "mimeType": "image/png", "data": png() }]}),
            &limits,
        )
        .expect("parsed");
        assert_eq!(r.dropped[0].code, "PART_TOO_LARGE");
    }

    #[test]
    fn an_unknown_block_kind_is_dropped_by_kind() {
        let r = parse_call_result(
            &json!({ "content": [
                { "type": "execute", "command": "rm -rf /" },
                { "type": "text", "text": "fine" },
            ]}),
            &Limits::default(),
        )
        .expect("parsed");
        assert_eq!(r.parts.len(), 1);
        assert_eq!(r.dropped[0].code, "PART_KIND_UNKNOWN");
        assert_eq!(r.dropped[0].kind, "execute");
    }

    #[test]
    fn an_embedded_resource_keeps_its_uri_without_being_fetched() {
        let r = parse_call_result(
            &json!({ "content": [{
                "type": "resource",
                "resource": { "uri": "file:///etc/passwd", "mimeType": "text/plain", "text": "root:x:0" },
            }]}),
            &Limits::default(),
        )
        .expect("parsed");
        match &r.parts[0] {
            Part::Resource { uri, text, .. } => {
                assert_eq!(uri, "file:///etc/passwd");
                assert_eq!(text.as_deref(), Some("root:x:0"));
            }
            other => panic!("expected a resource, got {other:?}"),
        }
    }

    #[test]
    fn structured_output_is_carried_and_a_shapeless_result_is_an_error() {
        let r = parse_call_result(
            &json!({ "content": [], "structuredContent": { "hits": 3 } }),
            &Limits::default(),
        )
        .expect("parsed");
        assert_eq!(r.structured.unwrap()["hits"], json!(3));
        assert_eq!(
            parse_call_result(&json!({}), &Limits::default())
                .unwrap_err()
                .code,
            "CALL_RESULT_MALFORMED"
        );
        assert_eq!(
            parse_call_result(&json!("done"), &Limits::default())
                .unwrap_err()
                .code,
            "CALL_RESULT_MALFORMED"
        );
    }
}
