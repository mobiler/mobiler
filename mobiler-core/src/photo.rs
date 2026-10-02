//! Per-call photo options for [`Cx::pick_photo_with`](crate::Cx::pick_photo_with) and
//! [`Cx::capture_photo_with`](crate::Cx::capture_photo_with): the format, size and metadata the
//! shell re-encodes a picked or captured image to (ADR-0045), and the typed reply. The pipeline
//! rules every shell follows live here as functions, so they are stated and tested once.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::PluginResponse;

/// The image format a photo is delivered in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PhotoFormat {
    /// Keep the picked file's format if it can pass through; when it must be re-encoded, PNG stays
    /// PNG and anything else (HEIC, WebP, …) becomes JPEG.
    #[default]
    Original,
    Jpeg,
    Png,
    /// Lossy WebP. iOS can't encode WebP (→ JPEG) and Safari's canvas can't (→ PNG); `Photo::mime`
    /// says what was produced.
    Webp,
}

impl PhotoFormat {
    pub fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Webp => "image/webp",
            Self::Jpeg | Self::Original => "image/jpeg",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Webp => "webp",
            Self::Jpeg | Self::Original => "jpg",
        }
    }

    fn is_original(&self) -> bool {
        *self == Self::Original
    }
}

/// Options for a picked or captured photo. Unset options keep the defaults: `Original` format, no
/// size limits, quality 85, metadata stripped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PhotoOptions {
    #[serde(skip_serializing_if = "PhotoFormat::is_original")]
    format: PhotoFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_dimension: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_bytes: Option<u64>,
    #[serde(skip_serializing_if = "is_default_quality")]
    quality: u8,
    #[serde(skip_serializing_if = "is_true")]
    strip_metadata: bool,
}

const DEFAULT_QUALITY: u8 = 85;

fn is_default_quality(q: &u8) -> bool {
    *q == DEFAULT_QUALITY
}

fn is_true(b: &bool) -> bool {
    *b
}

impl Default for PhotoOptions {
    fn default() -> Self {
        Self { format: PhotoFormat::Original, max_dimension: None, max_bytes: None, quality: DEFAULT_QUALITY, strip_metadata: true }
    }
}

impl PhotoOptions {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    #[must_use]
    pub fn jpeg(mut self) -> Self {
        self.format = PhotoFormat::Jpeg;
        self
    }
    #[must_use]
    pub fn png(mut self) -> Self {
        self.format = PhotoFormat::Png;
        self
    }
    #[must_use]
    pub fn webp(mut self) -> Self {
        self.format = PhotoFormat::Webp;
        self
    }
    #[must_use]
    pub fn original(mut self) -> Self {
        self.format = PhotoFormat::Original;
        self
    }
    /// The longest side in pixels; a smaller image is never upscaled.
    #[must_use]
    pub fn max_dimension(mut self, px: u32) -> Self {
        self.max_dimension = Some(px);
        self
    }
    /// The largest file allowed. A lossy format steps its quality down to fit; if it can't, the call
    /// fails with [`PhotoError::TooLarge`] rather than return a larger file.
    #[must_use]
    pub fn max_bytes(mut self, bytes: u64) -> Self {
        self.max_bytes = Some(bytes);
        self
    }
    /// The starting quality for a lossy format, 1–100 (default 85).
    #[must_use]
    pub fn quality(mut self, q: u8) -> Self {
        self.quality = q.clamp(1, 100);
        self
    }
    /// Keep the original's metadata when it can pass through untouched (default: stripped, which
    /// drops EXIF and the GPS position). A re-encoded image never carries metadata.
    #[must_use]
    pub fn keep_metadata(mut self) -> Self {
        self.strip_metadata = false;
        self
    }

    pub fn format(&self) -> PhotoFormat {
        self.format
    }
    pub fn max_dimension_opt(&self) -> Option<u32> {
        self.max_dimension
    }
    pub fn max_bytes_opt(&self) -> Option<u64> {
        self.max_bytes
    }
    pub fn quality_value(&self) -> u8 {
        self.quality.clamp(1, 100)
    }
    pub fn strip_metadata(&self) -> bool {
        self.strip_metadata
    }

    pub(crate) fn to_input(&self) -> String {
        serde_json::to_string(self).expect("serialize photo options")
    }

    /// Parse a `_with` call's input (for shells written in Rust); anything unparsable is the defaults.
    pub fn from_input(input: &str) -> Self {
        let mut o: Self = serde_json::from_str(input).unwrap_or_default();
        o.quality = o.quality.clamp(1, 100);
        o
    }
}

/// A delivered photo: a new file the shell wrote (`file://` on native, `blob:` on web) that
/// `image(…)` and `cx.upload` accept, with what the app needs to label an upload.
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Photo {
    pub handle: String,
    /// What the shell actually produced, e.g. `image/jpeg`.
    pub mime: String,
    pub bytes: u64,
    /// Upright dimensions (EXIF orientation applied).
    pub width: u32,
    pub height: u32,
}

impl Photo {
    /// Serialize for a `PluginResponse.output` (crux's FFI format, like [`crate::DeviceInfo`]).
    pub fn encode(&self) -> Vec<u8> {
        use crux_core::bridge::{BincodeFfiFormat, FfiFormat};
        let mut buffer = Vec::new();
        BincodeFfiFormat::serialize(&mut buffer, self).expect("encode Photo");
        buffer
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        use crux_core::bridge::{BincodeFfiFormat, FfiFormat};
        BincodeFfiFormat::deserialize(bytes).map_err(|e| e.to_string())
    }
}

/// Why a `_with` call delivered no photo. Word it yourself: the shells send only these codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhotoError {
    Cancelled,
    /// It could not be made to fit `max_bytes`.
    TooLarge,
    /// The platform can't decode it (e.g. HEIC on Android 8 or a non-Safari browser).
    UnsupportedImage,
    /// No picker or camera on this device.
    Unavailable,
    Failed(String),
}

impl PhotoError {
    pub fn code(&self) -> &str {
        match self {
            Self::Cancelled => "cancelled",
            Self::TooLarge => "too_large",
            Self::UnsupportedImage => "unsupported_image",
            Self::Unavailable => "unavailable",
            Self::Failed(text) => text,
        }
    }

    fn from_code(code: &str) -> Self {
        match code {
            "cancelled" => Self::Cancelled,
            "too_large" => Self::TooLarge,
            "unsupported_image" => Self::UnsupportedImage,
            "unavailable" => Self::Unavailable,
            other => Self::Failed(other.to_string()),
        }
    }
}

pub(crate) fn from_response(r: &PluginResponse) -> Result<Photo, PhotoError> {
    if r.ok {
        Photo::decode(&r.output).map_err(|_| PhotoError::Failed("unsupported shell".into()))
    } else {
        Err(PhotoError::from_code(&String::from_utf8_lossy(&r.output)))
    }
}

/// ADR-0045 rule: the original passes through untouched only when the format is `Original`, it fits
/// both limits, and there is no metadata to strip.
pub fn needs_reencode(format: PhotoFormat, fits: bool, strip_metadata: bool, has_metadata: bool) -> bool {
    format != PhotoFormat::Original || !fits || (strip_metadata && has_metadata)
}

/// ADR-0045 rule: scale so the longest (upright) side is at most `max`; never upscale.
pub fn target_size(width: u32, height: u32, max: Option<u32>) -> (u32, u32) {
    let longest = width.max(height);
    match max {
        Some(m) if m > 0 && longest > m => {
            let scale = f64::from(m) / f64::from(longest);
            let fit = |v: u32| ((f64::from(v) * scale).round() as u32).max(1);
            (fit(width), fit(height))
        }
        _ => (width, height),
    }
}

/// ADR-0045 rule: the qualities a lossy format tries for `max_bytes`: the start, then −10 to a floor of 45.
pub fn quality_ladder(start: u8) -> Vec<u8> {
    let start = start.clamp(1, 100);
    let mut steps = vec![start];
    let mut q = start;
    while q > 45 {
        q = q.saturating_sub(10).max(45);
        steps.push(q);
    }
    steps
}

/// ADR-0045 rule: the format a re-encode writes.
pub fn output_format(requested: PhotoFormat, source_is_png: bool) -> PhotoFormat {
    match requested {
        PhotoFormat::Original if source_is_png => PhotoFormat::Png,
        PhotoFormat::Original => PhotoFormat::Jpeg,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PluginResponse;

    #[test]
    fn options_json_omits_defaults_and_round_trips() {
        assert_eq!(PhotoOptions::new().to_input(), "{}");
        let o = PhotoOptions::new().jpeg().max_dimension(2048).max_bytes(4 << 20).quality(80).keep_metadata();
        assert_eq!(o.to_input(), r#"{"format":"jpeg","max_dimension":2048,"max_bytes":4194304,"quality":80,"strip_metadata":false}"#);
        assert_eq!(PhotoOptions::from_input(&o.to_input()), o);
        assert_eq!(PhotoOptions::from_input("{}"), PhotoOptions::new());
        assert_eq!(PhotoOptions::from_input("not json"), PhotoOptions::new());
        assert_eq!(PhotoOptions::new().quality(0).quality_value(), 1);
        assert_eq!(PhotoOptions::new().quality(200).quality_value(), 100);
    }

    #[test]
    fn photo_round_trips() {
        let p = Photo { handle: "file:///c/p.jpg".into(), mime: "image/jpeg".into(), bytes: 1_234_567, width: 2048, height: 1536 };
        assert_eq!(Photo::decode(&p.encode()), Ok(p));
    }

    #[test]
    fn codes_map_to_errors() {
        for (code, e) in [("cancelled", PhotoError::Cancelled), ("too_large", PhotoError::TooLarge), ("unsupported_image", PhotoError::UnsupportedImage), ("unavailable", PhotoError::Unavailable)] {
            assert_eq!(from_response(&PluginResponse::text(false, code)), Err(e.clone()));
            assert_eq!(e.code(), code);
        }
        assert_eq!(from_response(&PluginResponse::text(false, "disk full")), Err(PhotoError::Failed("disk full".into())));
    }

    #[test]
    fn ok_reply_that_is_not_a_photo_is_failed() {
        // An old shell ignores the options and answers a plain URI.
        assert_eq!(from_response(&PluginResponse::text(true, "content://media/1")), Err(PhotoError::Failed("unsupported shell".into())));
    }

    #[test]
    fn pass_through_only_when_original_fits_and_clean() {
        assert!(!needs_reencode(PhotoFormat::Original, true, true, false));
        assert!(!needs_reencode(PhotoFormat::Original, true, false, true));
        assert!(needs_reencode(PhotoFormat::Original, true, true, true));
        assert!(needs_reencode(PhotoFormat::Original, false, false, false));
        assert!(needs_reencode(PhotoFormat::Jpeg, true, false, false));
    }

    #[test]
    fn target_size_caps_the_longest_side_and_never_upscales() {
        assert_eq!(target_size(4000, 3000, Some(2048)), (2048, 1536));
        assert_eq!(target_size(3000, 4000, Some(2048)), (1536, 2048));
        assert_eq!(target_size(800, 600, Some(2048)), (800, 600));
        assert_eq!(target_size(800, 600, None), (800, 600));
        assert_eq!(target_size(10_000, 1, Some(100)), (100, 1));
    }

    #[test]
    fn quality_ladder_steps_down_to_45() {
        assert_eq!(quality_ladder(85), vec![85, 75, 65, 55, 45]);
        assert_eq!(quality_ladder(50), vec![50, 45]);
        assert_eq!(quality_ladder(30), vec![30]);
        assert!(quality_ladder(100).iter().all(|q| *q >= 45 || *q == 100));
    }

    #[test]
    fn original_keeps_png_and_writes_everything_else_as_jpeg() {
        assert_eq!(output_format(PhotoFormat::Original, true), PhotoFormat::Png);
        assert_eq!(output_format(PhotoFormat::Original, false), PhotoFormat::Jpeg);
        assert_eq!(output_format(PhotoFormat::Webp, false), PhotoFormat::Webp);
        assert_eq!((PhotoFormat::Jpeg.mime(), PhotoFormat::Jpeg.extension()), ("image/jpeg", "jpg"));
    }
}
