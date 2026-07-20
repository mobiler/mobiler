//! Streaming file transfers (`cx.upload` / `cx.download`).
//!
//! Large transfers move by string **handle** (a filesystem path, a `content://` /
//! `file://` URI, or a web `blob:` URL) — the bytes never cross the FFI. Each transfer
//! rides the streaming primitive ([`Cx::subscribe`](crate::Cx::subscribe)) and delivers
//! a [`TransferEvent`] per progress tick and once at completion.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::http::HttpOutcome;

/// One event from an in-flight transfer, bincoded into the stream's
/// `PluginResponse.output` (the same pattern Release A uses for [`HttpOutcome`]).
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum TransferEvent {
    /// A progress tick. `total` is `None` when the size is unknown (chunked response).
    Progress { transferred: u64, total: Option<u64> },
    /// The transfer finished. `outcome` is Release A's request result; for a download
    /// its `body` is empty (bytes went to disk) and `handle` is the destination the
    /// shell wrote (sandbox path on native, `blob:` URL on web). `handle` is `None` for
    /// an upload.
    Done { outcome: HttpOutcome, handle: Option<String> },
}

impl TransferEvent {
    /// Serialize for the stream payload. Uses crux's FFI format, not bincode directly —
    /// see [`HttpOutcome::encode`](crate::http::HttpOutcome::encode) for why the version
    /// and config matter.
    pub fn encode(&self) -> Vec<u8> {
        use crux_core::bridge::{BincodeFfiFormat, FfiFormat};
        let mut buffer = Vec::new();
        BincodeFfiFormat::serialize(&mut buffer, self).expect("encode TransferEvent");
        buffer
    }

    /// Decode a stream payload produced by a shell's `transfer` plugin.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        use crux_core::bridge::{BincodeFfiFormat, FfiFormat};
        BincodeFfiFormat::deserialize(bytes).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{HttpHeader, HttpOutcome};

    #[test]
    fn round_trips_progress_with_and_without_total() {
        for ev in [
            TransferEvent::Progress { transferred: 0, total: Some(1024) },
            TransferEvent::Progress { transferred: 999_999_999, total: None },
        ] {
            assert_eq!(TransferEvent::decode(&ev.encode()).unwrap(), ev);
        }
    }

    #[test]
    fn round_trips_done_upload_and_download() {
        let up = TransferEvent::Done {
            outcome: HttpOutcome::Response { status: 200, headers: vec![], body: vec![] },
            handle: None,
        };
        let down = TransferEvent::Done {
            outcome: HttpOutcome::Response {
                status: 200,
                headers: vec![HttpHeader { name: "Content-Length".into(), value: "5".into() }],
                body: vec![],
            },
            handle: Some("blob:abc".into()),
        };
        for ev in [up, down] {
            assert_eq!(TransferEvent::decode(&ev.encode()).unwrap(), ev);
        }
    }

    #[test]
    fn decode_rejects_garbage_without_panicking() {
        assert!(TransferEvent::decode(&[0xff, 0xff, 0xff]).is_err());
    }
}
