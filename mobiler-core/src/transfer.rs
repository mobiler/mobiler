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

use crate::{Cx, HttpHeader, PluginResponse};

/// Multipart/form-data config for an upload. When present, the shell builds a
/// `multipart/form-data` body: the text `fields` (in order) then the file part LAST.
#[derive(Serialize)]
struct Multipart {
    /// Form field name for the file part.
    field: String,
    /// Override the file part's `filename=`; else the shell infers it from the source.
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    /// Override the file part's `Content-Type`; else `application/octet-stream`.
    #[serde(skip_serializing_if = "Option::is_none")]
    file_content_type: Option<String>,
    /// Text fields (reuse HttpHeader's name/value).
    fields: Vec<HttpHeader>,
}

/// Wire shape of a transfer request, serialized into the stream call's `input`.
/// Exactly one of `source` (upload) / `dest` (download) is set.
#[derive(Serialize)]
struct TransferReq {
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    multipart: Option<Multipart>,
    headers: Vec<HttpHeader>,
}

/// Builds a streaming transfer. Obtained from [`Cx::upload`] / [`Cx::download`];
/// finished with [`start`](Self::start), which subscribes and returns the key.
pub struct TransferBuilder<'a, E> {
    cx: &'a mut Cx<E>,
    op: &'static str, // "upload" | "download"
    req: TransferReq,
    method_set_by_caller: bool,
}

impl<'a, E> TransferBuilder<'a, E> {
    pub(crate) fn upload(cx: &'a mut Cx<E>, url: String, source: String) -> Self {
        Self {
            cx,
            op: "upload",
            req: TransferReq {
                url,
                source: Some(source),
                dest: None,
                method: Some("PUT".into()),
                multipart: None,
                headers: Vec::new(),
            },
            method_set_by_caller: false,
        }
    }

    pub(crate) fn download(cx: &'a mut Cx<E>, url: String, dest: String) -> Self {
        Self {
            cx,
            op: "download",
            req: TransferReq {
                url,
                source: None,
                dest: Some(dest),
                method: None,
                multipart: None,
                headers: Vec::new(),
            },
            method_set_by_caller: false,
        }
    }

    /// Override the upload method (default `PUT`). No effect on download.
    #[must_use]
    pub fn method(mut self, m: impl Into<String>) -> Self {
        if self.op == "upload" {
            self.req.method = Some(m.into());
            self.method_set_by_caller = true;
        }
        self
    }

    /// Send as `multipart/form-data`: the file becomes a part named `field`, emitted after
    /// any text `field()`s. Flips the default method to POST (an explicit `method()` wins).
    /// Upload-only.
    #[must_use]
    pub fn multipart(mut self, field: impl Into<String>) -> Self {
        if self.op == "upload" {
            if !self.method_set_by_caller {
                self.req.method = Some("POST".into());
            }
            match &mut self.req.multipart {
                Some(m) => m.field = field.into(),
                None => {
                    self.req.multipart = Some(Multipart {
                        field: field.into(),
                        filename: None,
                        file_content_type: None,
                        fields: Vec::new(),
                    })
                }
            }
        }
        self
    }

    /// Add a text field to the multipart body (order preserved). No effect unless
    /// `multipart()` was called; no effect on download.
    #[must_use]
    pub fn field(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        if let Some(m) = &mut self.req.multipart {
            m.fields.push(HttpHeader { name: name.into(), value: value.into() });
        }
        self
    }

    /// Override the multipart file part's `filename=` (default: inferred from the source).
    #[must_use]
    pub fn filename(mut self, name: impl Into<String>) -> Self {
        if let Some(m) = &mut self.req.multipart {
            m.filename = Some(name.into());
        }
        self
    }

    /// Override the multipart file part's `Content-Type` (default: `application/octet-stream`).
    #[must_use]
    pub fn file_content_type(mut self, ct: impl Into<String>) -> Self {
        if let Some(m) = &mut self.req.multipart {
            m.file_content_type = Some(ct.into());
        }
        self
    }

    /// Add a request header (order preserved, repeats allowed).
    #[must_use]
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.req.headers.push(HttpHeader { name: name.into(), value: value.into() });
        self
    }

    /// Sugar for `header("Authorization", format!("Bearer {token}"))`.
    #[must_use]
    pub fn bearer(self, token: impl AsRef<str>) -> Self {
        self.header("Authorization", format!("Bearer {}", token.as_ref()))
    }

    /// Subscribe under `key`. `on_event` fires per progress tick and once at `Done`.
    /// Returns `key` so the caller can [`cx.unsubscribe(key)`](crate::Cx::unsubscribe).
    pub fn start(self, key: impl Into<String>, on_event: impl Fn(TransferEvent) -> E + Send + 'static) -> String {
        let key = key.into();
        let input = serde_json::to_string(&self.req).expect("serialize transfer request");
        self.cx.subscribe(key.clone(), "transfer", self.op, input, move |r: PluginResponse| {
            on_event(decode_event(&r))
        });
        key
    }
}

/// Decode a stream payload. A shell that emits something undecodable is a bug, but it
/// must not panic the app — surface it as a completed transport error.
fn decode_event(r: &PluginResponse) -> TransferEvent {
    TransferEvent::decode(&r.output).unwrap_or_else(|e| TransferEvent::Done {
        outcome: HttpOutcome::TransportError { message: format!("malformed transfer event: {e}") },
        handle: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{HttpHeader, HttpOutcome};
    use serde::Serialize;

    #[derive(Serialize)]
    enum Ev {
        Tap,
    }

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

    #[test]
    fn multipart_sets_config_and_flips_method_to_post() {
        let mut cx = Cx::<Ev>::default();
        cx.upload("https://h/up", "file:///tmp/a.jpg")
            .multipart("file")
            .field("title", "My Photo")
            .field("album", "vac")
            .start("m-1", |_| Ev::Tap);

        let (call, _) = &cx.streams[0];
        let v: serde_json::Value = serde_json::from_str(&call.input).unwrap();
        assert_eq!(v["method"], "POST", "multipart flips default PUT -> POST");
        assert_eq!(v["multipart"]["field"], "file");
        assert_eq!(v["multipart"]["fields"][0]["name"], "title");
        assert_eq!(v["multipart"]["fields"][0]["value"], "My Photo");
        assert_eq!(v["multipart"]["fields"][1]["name"], "album");
        // filename / file_content_type omitted when not overridden
        assert!(v["multipart"].get("filename").is_none());
        assert!(v["multipart"].get("file_content_type").is_none());
    }

    #[test]
    fn explicit_method_survives_multipart() {
        let mut cx = Cx::<Ev>::default();
        cx.upload("https://h/up", "file:///tmp/a.jpg")
            .method("PUT")
            .multipart("file")
            .start("m-2", |_| Ev::Tap);
        let (call, _) = &cx.streams[0];
        let v: serde_json::Value = serde_json::from_str(&call.input).unwrap();
        assert_eq!(v["method"], "PUT", "an explicit .method() is not overridden by .multipart()");
    }

    #[test]
    fn multipart_overrides_land_in_config() {
        let mut cx = Cx::<Ev>::default();
        cx.upload("https://h/up", "file:///tmp/a.bin")
            .multipart("f")
            .filename("photo.jpg")
            .file_content_type("image/jpeg")
            .start("m-3", |_| Ev::Tap);
        let (call, _) = &cx.streams[0];
        let v: serde_json::Value = serde_json::from_str(&call.input).unwrap();
        assert_eq!(v["multipart"]["filename"], "photo.jpg");
        assert_eq!(v["multipart"]["file_content_type"], "image/jpeg");
    }

    #[test]
    fn multipart_is_a_noop_on_download() {
        let mut cx = Cx::<Ev>::default();
        cx.download("https://h/get", "/d").multipart("f").field("k", "v").start("d-1", |_| Ev::Tap);
        let (call, _) = &cx.streams[0];
        let v: serde_json::Value = serde_json::from_str(&call.input).unwrap();
        assert!(v.get("multipart").is_none(), "download ignores multipart");
        assert!(v.get("method").is_none(), "download has no method");
    }
}
