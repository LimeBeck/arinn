// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native Messaging framing: a 32-bit length in native byte order, then UTF-8
//! JSON. The browser accepts at most 1 MiB from the host; the host accepts at
//! most [`MAX_INCOMING`] from the extension.
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const MAX_OUTGOING: usize = 1024 * 1024;
pub const MAX_INCOMING: usize = 8 * 1024 * 1024;

/// Read one frame. `Ok(None)` is a clean end of input between frames.
pub async fn read<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut length = [0u8; 4];
    match reader.read_exact(&mut length).await {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let length = u32::from_ne_bytes(length) as usize;
    if length > MAX_INCOMING {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await?;
    Ok(Some(body))
}

pub async fn write<W: AsyncWrite + Unpin>(writer: &mut W, body: &[u8]) -> io::Result<()> {
    if body.len() > MAX_OUTGOING {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "frame too large",
        ));
    }
    writer.write_all(&(body.len() as u32).to_ne_bytes()).await?;
    writer.write_all(body).await?;
    writer.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn frames_round_trip_and_enforce_limits() {
        let mut buffer = Vec::new();
        write(&mut buffer, b"{\"a\":1}").await.unwrap();
        write(&mut buffer, b"").await.unwrap();
        let mut reader = buffer.as_slice();
        assert_eq!(read(&mut reader).await.unwrap().unwrap(), b"{\"a\":1}");
        assert_eq!(read(&mut reader).await.unwrap().unwrap(), b"");
        assert!(read(&mut reader).await.unwrap().is_none());
        // A truncated frame is an error, not a clean end.
        let mut truncated: &[u8] = &[5, 0, 0, 0, b'x'];
        assert!(read(&mut truncated).await.is_err());
        let huge = ((MAX_INCOMING + 1) as u32).to_ne_bytes();
        assert!(read(&mut huge.as_slice()).await.is_err());
        assert!(
            write(&mut Vec::new(), &vec![0; MAX_OUTGOING + 1])
                .await
                .is_err()
        );
    }
}
