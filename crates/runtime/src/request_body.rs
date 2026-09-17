//! Bounded inspection of compressed requests; original transport bytes stay intact.
use axum::{
    body::Bytes,
    http::{HeaderMap, StatusCode},
};
use std::{
    io::Read,
    sync::{Arc, LazyLock},
};
use tokio::sync::Semaphore;

pub(crate) const MAX_DECODED: usize = 32 * 1024 * 1024;
static DECODERS: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(4)));

pub(crate) async fn decode(headers: &HeaderMap, bytes: Bytes) -> Result<Bytes, StatusCode> {
    let mut encodings = headers.get_all("content-encoding").iter();
    let Some(encoding) = encodings.next() else {
        return Ok(bytes);
    };
    let encoding = encoding
        .to_str()
        .map_err(|_| StatusCode::UNSUPPORTED_MEDIA_TYPE)?
        .trim();
    if encodings.next().is_some() {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
    if encoding.eq_ignore_ascii_case("identity") {
        return Ok(bytes);
    }
    if !encoding.eq_ignore_ascii_case("zstd") {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
    let permit = DECODERS
        .clone()
        .try_acquire_owned()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    tokio::task::spawn_blocking(move || {
        // The worker keeps its permit even when the HTTP client disappears.
        let _permit = permit;
        decode_zstd(&bytes, MAX_DECODED).map(Bytes::from)
    })
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
}

fn decode_zstd(bytes: &[u8], limit: usize) -> Result<Vec<u8>, StatusCode> {
    let mut decoder =
        zstd::stream::read::Decoder::new(bytes).map_err(|_| StatusCode::BAD_REQUEST)?;
    decoder
        .window_log_max(25)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let mut output = Vec::new();
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let read = decoder
            .read(&mut buffer)
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        if read == 0 {
            break;
        }
        if read > limit.saturating_sub(output.len()) {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
        output.extend_from_slice(&buffer[..read]);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoding_checks_full_stream_and_bounds_expansion() {
        let compressed = zstd::stream::encode_all(&b"fixture payload"[..], 1).unwrap();
        assert_eq!(decode_zstd(&compressed, 15).unwrap(), b"fixture payload");
        assert_eq!(
            decode_zstd(&compressed, 14).unwrap_err(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(
            decode_zstd(&compressed[..compressed.len() - 1], 32).unwrap_err(),
            StatusCode::BAD_REQUEST
        );
        let mut trailing = compressed.clone();
        trailing.extend_from_slice(b"invalid trailer");
        assert_eq!(
            decode_zstd(&trailing, 32).unwrap_err(),
            StatusCode::BAD_REQUEST
        );
        let mut combined = compressed.clone();
        combined.extend_from_slice(&compressed);
        assert_eq!(
            decode_zstd(&combined, 30).unwrap(),
            b"fixture payloadfixture payload"
        );
        assert_eq!(
            decode_zstd(&combined, 29).unwrap_err(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }

    #[tokio::test]
    async fn ambiguous_or_unsupported_encodings_fail_without_guessing() {
        for encoding in ["gzip", "zstd, identity", ""] {
            let mut headers = HeaderMap::new();
            headers.insert("content-encoding", encoding.parse().unwrap());
            assert_eq!(
                decode(&headers, Bytes::new()).await.unwrap_err(),
                StatusCode::UNSUPPORTED_MEDIA_TYPE
            );
        }
        let mut headers = HeaderMap::new();
        headers.append("content-encoding", "identity".parse().unwrap());
        headers.append("content-encoding", "identity".parse().unwrap());
        assert_eq!(
            decode(&headers, Bytes::new()).await.unwrap_err(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
    }
}
