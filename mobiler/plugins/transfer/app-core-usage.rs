// Rust app-side usage of the transfer plugin (drop into shared/src/app.rs).
// Rides the streaming primitive via the `cx.upload`/`cx.download` builders: `start` subscribes and
// delivers a Msg::Xfer per progress tick + once at completion (no app-driven poll loop);
// `cx.unsubscribe` cancels an in-flight transfer.

use mobiler_core::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub enum Msg {
    StartUpload,
    StartDownload,
    Xfer(TransferEvent),
    Cancel,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Model {
    pub busy: bool,
    pub percent: Option<f64>,
    pub saved_at: Option<String>,
    pub error: Option<String>,
}

impl MyApp {
    fn handle(&self, msg: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match msg {
            // Upload a sandbox file as the request body (PUT by default) — one Msg::Xfer per
            // progress tick and once at completion, until Done or cancel.
            Msg::StartUpload => {
                model.busy = true;
                model.percent = Some(0.0);
                model.error = None;
                cx.upload("https://example.com/upload", "receipts/2026-07.jpg")
                    .bearer("token123")
                    .start("xfer", Msg::Xfer);
            }
            // Download to a sandbox-relative path — same event shape as upload, but `handle` on
            // Done carries the path the shell wrote the bytes to.
            Msg::StartDownload => {
                model.busy = true;
                model.percent = Some(0.0);
                model.error = None;
                cx.download("https://example.com/report.pdf", "downloads/report.pdf")
                    .start("xfer", Msg::Xfer);
            }
            Msg::Xfer(TransferEvent::Progress { transferred, total }) => {
                model.percent = total.map(|t| transferred as f64 / t.max(1) as f64 * 100.0);
            }
            Msg::Xfer(TransferEvent::Done { outcome, handle }) => {
                model.busy = false;
                match outcome {
                    HttpOutcome::Response { status, .. } if (200..300).contains(&status) => {
                        model.saved_at = handle; // Some(path) for a download; None for an upload
                    }
                    HttpOutcome::Response { status, .. } => {
                        model.error = Some(format!("server rejected the transfer: http {status}"));
                    }
                    HttpOutcome::TransportError { message } => {
                        model.error = Some(message);
                    }
                }
            }
            // Cancel a transfer in flight. No terminal Msg::Xfer follows a cancel — the shell
            // stays silent (see the plugin's README for why), so reset local state here instead.
            Msg::Cancel => {
                cx.unsubscribe("xfer");
                model.busy = false;
                model.percent = None;
            }
        }
    }
}
