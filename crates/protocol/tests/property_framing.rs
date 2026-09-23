//! M9.6 (docs/52 "Security gates": fuzzer for the protocol decoder). Arbitrary
//! bytes never panic the frame reader; a declared length above the ceiling is
//! refused before any allocation of that size; every frame the writer emits
//! reads back identical; and the length prefix is load-bearing (docs/55): a
//! reader that trusted a corrupted prefix would read garbage or hang.

use modbit_protocol::framing::{FrameError, MAX_FRAME_BYTES, read_frame, write_frame};
use modbit_protocol::v1::{CommandEnvelope, Id, SurfaceFrame, surface_frame};
use proptest::prelude::*;
use std::io::Cursor;

fn frame() -> impl Strategy<Value = SurfaceFrame> {
    (
        prop::collection::vec(any::<u8>(), 0..32),
        "[A-Za-z]{0,24}",
        prop::collection::vec(any::<u8>(), 0..2048),
        any::<u32>(),
        prop::option::of(any::<u64>()),
    )
        .prop_map(|(id, ty, payload, schema, generation)| SurfaceFrame {
            body: Some(surface_frame::Body::Command(CommandEnvelope {
                command_id: Some(Id { value: id.clone() }),
                tenant_id: Some(Id { value: id.clone() }),
                user_id: Some(Id { value: id }),
                session_id: None,
                aggregate_id: None,
                expected_generation: generation,
                command_type: ty,
                schema_version: schema,
                payload,
                issued_at: None,
            })),
        })
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn arbitrary_bytes_never_panic_the_reader(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        let r = rt().block_on(async {
            let mut c = Cursor::new(bytes);
            read_frame(&mut c).await
        });
        // Any of the three outcomes is acceptable; a panic is not.
        let _ = r;
    }

    #[test]
    fn a_declared_length_above_the_ceiling_is_refused_before_reading_it(
        extra in 1u32..=u32::MAX - (MAX_FRAME_BYTES as u32),
        trailing in prop::collection::vec(any::<u8>(), 0..8),
    ) {
        let declared = MAX_FRAME_BYTES as u32 + extra;
        let mut bytes = declared.to_be_bytes().to_vec();
        bytes.extend(trailing);
        let r = rt().block_on(async {
            let mut c = Cursor::new(bytes);
            read_frame(&mut c).await
        });
        prop_assert!(matches!(r, Err(FrameError::TooLarge { declared: d, .. }) if d == declared as usize), "{r:?}");
    }

    #[test]
    fn every_written_frame_reads_back_identical_and_a_corrupted_prefix_does_not(f in frame()) {
        let (back, corrupted) = rt().block_on(async {
            let mut w = Vec::new();
            write_frame(&mut w, &f).await.unwrap();
            let back = read_frame(&mut Cursor::new(w.clone())).await.unwrap();
            // docs/55: the prefix is the control — declare one byte more than
            // was written and the read must fail, never return a frame.
            let mut bad = w.clone();
            let len = u32::from_be_bytes([bad[0], bad[1], bad[2], bad[3]]) + 1;
            bad[..4].copy_from_slice(&len.to_be_bytes());
            let corrupted = read_frame(&mut Cursor::new(bad)).await;
            (back, corrupted)
        });
        prop_assert_eq!(back, Some(f));
        prop_assert!(corrupted.is_err(), "{corrupted:?}");
    }
}
