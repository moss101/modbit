//! Length-prefixed framing for the local SurfaceProtocol (docs/30): a 4-byte
//! big-endian length followed by one encoded `SurfaceFrame`. Frames above
//! [`MAX_FRAME_BYTES`] are rejected before allocation on both sides
//! (REQ-EV-0108: no giant IPC body; large payloads go by OutputRef).

use prost::Message;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::v1::SurfaceFrame;

/// Write any length-prefixed protobuf message (shared by the execd protocol).
pub async fn write_message<W: AsyncWrite + Unpin, M: Message>(
    w: &mut W,
    msg: &M,
) -> Result<(), FrameError> {
    let bytes = msg.encode_to_vec();
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge {
            declared: bytes.len(),
            max: MAX_FRAME_BYTES,
        });
    }
    let len = u32::try_from(bytes.len()).expect("bounded above");
    w.write_all(&len.to_be_bytes()).await?;
    w.write_all(&bytes).await?;
    w.flush().await?;
    Ok(())
}

/// Read any length-prefixed protobuf message; `Ok(None)` on clean EOF.
pub async fn read_message<R: AsyncRead + Unpin, M: Message + Default>(
    r: &mut R,
) -> Result<Option<M>, FrameError> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let declared = u32::from_be_bytes(len) as usize;
    if declared > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge {
            declared,
            max: MAX_FRAME_BYTES,
        });
    }
    let mut buf = vec![0u8; declared];
    r.read_exact(&mut buf).await?;
    Ok(Some(M::decode(buf.as_slice())?))
}

/// Hard ceiling for one frame.
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

/// Framing errors.
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    /// I/O failure or peer closed.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// Declared length exceeds the ceiling.
    #[error("frame of {declared} bytes exceeds the {max}-byte ceiling")]
    TooLarge {
        /// Declared length.
        declared: usize,
        /// Ceiling.
        max: usize,
    },
    /// Bytes do not decode as a `SurfaceFrame`.
    #[error("malformed frame: {0}")]
    Malformed(#[from] prost::DecodeError),
    /// A frame with no body.
    #[error("empty frame body")]
    EmptyBody,
}

/// Write one frame.
pub async fn write_frame<W: AsyncWrite + Unpin>(
    w: &mut W,
    frame: &SurfaceFrame,
) -> Result<(), FrameError> {
    let bytes = frame.encode_to_vec();
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge {
            declared: bytes.len(),
            max: MAX_FRAME_BYTES,
        });
    }
    let len = u32::try_from(bytes.len()).expect("bounded above");
    w.write_all(&len.to_be_bytes()).await?;
    w.write_all(&bytes).await?;
    w.flush().await?;
    Ok(())
}

/// Read one frame; `Ok(None)` on a clean EOF before any length byte.
pub async fn read_frame<R: AsyncRead + Unpin>(
    r: &mut R,
) -> Result<Option<SurfaceFrame>, FrameError> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let declared = u32::from_be_bytes(len) as usize;
    if declared > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge {
            declared,
            max: MAX_FRAME_BYTES,
        });
    }
    let mut buf = vec![0u8; declared];
    r.read_exact(&mut buf).await?;
    let frame = SurfaceFrame::decode(buf.as_slice())?;
    if frame.body.is_none() {
        return Err(FrameError::EmptyBody);
    }
    Ok(Some(frame))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::{ProtocolError, surface_frame::Body};

    #[tokio::test]
    async fn round_trips_and_rejects_oversized_or_malformed_frames() {
        let frame = SurfaceFrame {
            body: Some(Body::Error(ProtocolError {
                code: "X".into(),
                message: "y".into(),
            })),
        };
        let mut buf = Vec::new();
        write_frame(&mut buf, &frame).await.unwrap();
        let mut cursor = std::io::Cursor::new(buf.clone());
        assert_eq!(read_frame(&mut cursor).await.unwrap(), Some(frame));
        assert_eq!(read_frame(&mut cursor).await.unwrap(), None);

        let mut huge = ((MAX_FRAME_BYTES + 1) as u32).to_be_bytes().to_vec();
        huge.extend_from_slice(&[0; 8]);
        let err = read_frame(&mut std::io::Cursor::new(huge))
            .await
            .unwrap_err();
        assert!(matches!(err, FrameError::TooLarge { .. }), "{err}");

        let mut bad = 3u32.to_be_bytes().to_vec();
        bad.extend_from_slice(&[0xff, 0xff, 0xff]);
        let err = read_frame(&mut std::io::Cursor::new(bad))
            .await
            .unwrap_err();
        assert!(matches!(err, FrameError::Malformed(_)), "{err}");

        let empty = 0u32.to_be_bytes().to_vec();
        let err = read_frame(&mut std::io::Cursor::new(empty))
            .await
            .unwrap_err();
        assert!(matches!(err, FrameError::EmptyBody), "{err}");
    }
}
