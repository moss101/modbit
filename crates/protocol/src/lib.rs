//! `modbit-protocol` — local/cloud framing and generated schemas (SurfaceProtocol wire types).
//!
//! Canonical owner: core-runtime owner (`docs/12_REPOSITORY_AND_MODULE_LAYOUT.md`).
//! Schema source: `proto/modbit/v1/*.proto` (docs/30). Rust bindings are
//! generated at build time (M0.3); TypeScript bindings for the same files live
//! in `packages/surface-protocol` and are proven equivalent by the
//! cross-language round-trip fixtures in `tools/protocol-fixtures`.
//!
//! Transport framing, authentication and command handling arrive with M1.3;
//! nothing here dispatches a command.

/// Protocol major version 1 message types, generated from `proto/modbit/v1`.
#[allow(missing_docs, clippy::all)]
pub mod v1 {
    include!(concat!(env!("OUT_DIR"), "/modbit.v1.rs"));
}

/// Serialized `FileDescriptorSet` of the compiled schema, for reflection and
/// compatibility checks.
pub const FILE_DESCRIPTOR_SET: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/modbit_v1_descriptor.bin"));

/// The protocol version this build speaks.
pub const PROTOCOL_VERSION: v1::ProtocolVersion = v1::ProtocolVersion { major: 1, minor: 0 };

#[cfg(test)]
mod tests {
    use super::v1::*;
    use prost::Message;

    fn id(b: u8) -> Id {
        Id { value: vec![b; 16] }
    }

    #[test]
    fn command_envelope_round_trips_binary() {
        let msg = CommandEnvelope {
            command_id: Some(id(1)),
            tenant_id: Some(id(2)),
            user_id: Some(id(3)),
            session_id: Some(id(4)),
            aggregate_id: None,
            expected_generation: Some(7),
            command_type: "CreateTask".into(),
            schema_version: 1,
            payload: b"\x01\x02".to_vec(),
            issued_at: Some(prost_types::Timestamp {
                seconds: 1_700_000_000,
                nanos: 5,
            }),
        };
        let bytes = msg.encode_to_vec();
        let back = CommandEnvelope::decode(bytes.as_slice()).unwrap();
        assert_eq!(back, msg);
        assert_eq!(back.encode_to_vec(), bytes);
    }

    #[test]
    fn unknown_additive_field_is_tolerated() {
        // Forward compatibility (docs/30): a newer writer may add field 99.
        let mut bytes = ToolCallRequest {
            tool_name: "fs.read".into(),
            timeout_ms: 5,
            ..Default::default()
        }
        .encode_to_vec();
        bytes.extend_from_slice(&[(99 << 3) | 2, 3, b'x', b'y', b'z']); // field 99, length-delimited "xyz"
        let back = ToolCallRequest::decode(bytes.as_slice()).unwrap();
        assert_eq!(back.tool_name, "fs.read");
        assert_eq!(back.timeout_ms, 5);
    }

    #[test]
    fn descriptor_set_names_every_v1_file() {
        let fds = prost_types::FileDescriptorSet::decode(super::FILE_DESCRIPTOR_SET).unwrap();
        let names: Vec<_> = fds.file.iter().map(|f| f.name().to_owned()).collect();
        for f in ["domain", "envelope", "tool", "output_ref", "negotiation"] {
            assert!(
                names.iter().any(|n| n == &format!("modbit/v1/{f}.proto")),
                "{names:?}"
            );
        }
        assert_eq!(super::PROTOCOL_VERSION.major, 1);
    }
}
