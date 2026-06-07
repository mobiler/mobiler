//! Bunny.net Stream URL helpers — pure string builders, no I/O.
//!
//! Bunny is **not** a special widget in Mobiler; it is just URLs fed into the two general video
//! surfaces:
//! - the **embed iframe URL** ([`embed_url`] / [`embed_url_signed`]) → a [`web_view`](crate::web_view)
//!   widget, which renders Bunny's own player (adaptive HLS + captions + quality + thumbnails, free,
//!   on every platform);
//! - the **direct HLS URL** ([`hls_url`]) → a [`video_player`](crate::video_player) widget, the
//!   controllable native player (AVPlayer / Media3 / `<video>`).
//!
//! # ⚠️ Secret-key safety
//! [`embed_url_signed`] hashes your **Token Authentication Key**, which is a **secret**. Never call
//! it from a shipped client — the key is extractable from the binary, and native apps have no
//! referrer/domain to fall back on. Run it **server-side** (this crate compiles for your Rust
//! backend too) and pass the finished, time-boxed URL to the app via your model. On the client,
//! default to the **unsigned** builders ([`embed_url`] / [`hls_url`]) with Bunny's domain/referrer
//! allowlist, or a backend-signed URL.
//!
//! Protecting the **direct** URL (the HLS playlist + its segments) needs Bunny's *Advanced Token
//! Authentication* (HMAC-SHA256 directory tokens), which is server-side-only and zone-specific — do
//! that in your backend (see <https://docs.bunny.net/docs/cdn-token-authentication-advanced>) and
//! feed the signed URL to [`video_player`](crate::video_player). For protected playback the simplest
//! path is the signed **embed** ([`embed_url_signed`]) rendered in a [`web_view`](crate::web_view).

use sha2::{Digest, Sha256};

const EMBED_BASE: &str = "https://iframe.mediadelivery.net/embed";

/// The unsigned Bunny embed-player URL for `video_id` in `library_id`, e.g.
/// `https://iframe.mediadelivery.net/embed/12345/abc-123`. Render it in a
/// [`web_view`](crate::web_view). Use when the video is public or protected by a Bunny
/// domain/referrer allowlist.
#[must_use]
pub fn embed_url(library_id: impl AsRef<str>, video_id: impl AsRef<str>) -> String {
    format!("{EMBED_BASE}/{}/{}", library_id.as_ref(), video_id.as_ref())
}

/// The **token-secured** Bunny embed-player URL: appends `?token=<hex>&expires=<unix_seconds>` where
/// `token = SHA256_HEX(token_security_key + video_id + expires)` (Bunny *Embed View Token
/// Authentication*). `expires` is a UNIX timestamp **in seconds**.
///
/// # ⚠️ Server-side only
/// `token_security_key` is a secret — see the [module docs](self). Call this in your backend and
/// pass the result to the client; do not embed the key in a shipped app.
#[must_use]
pub fn embed_url_signed(
    library_id: impl AsRef<str>,
    video_id: impl AsRef<str>,
    token_security_key: impl AsRef<str>,
    expires_unix_seconds: i64,
) -> String {
    let video_id = video_id.as_ref();
    let token = embed_token(token_security_key.as_ref(), video_id, expires_unix_seconds);
    format!(
        "{EMBED_BASE}/{}/{video_id}?token={token}&expires={expires_unix_seconds}",
        library_id.as_ref()
    )
}

/// The Bunny *Embed View Token* for a video: `SHA256_HEX(token_security_key + video_id + expires)`.
/// Exposed for callers that build the URL themselves; prefer [`embed_url_signed`].
#[must_use]
pub fn embed_token(token_security_key: &str, video_id: &str, expires_unix_seconds: i64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token_security_key.as_bytes());
    hasher.update(video_id.as_bytes());
    hasher.update(expires_unix_seconds.to_string().as_bytes());
    hex::encode(hasher.finalize())
}

/// The unsigned **direct** HLS manifest URL for `video_id`, served from your Stream pull-zone host
/// (`pull_zone_host` = e.g. `vz-xxxx.b-cdn.net`, no scheme): `https://<host>/<video_id>/playlist.m3u8`.
/// Feed it to a [`video_player`](crate::video_player) — the controllable native player. For protected
/// direct playback, sign it with Bunny Advanced Token Authentication server-side (see the
/// [module docs](self)).
#[must_use]
pub fn hls_url(pull_zone_host: impl AsRef<str>, video_id: impl AsRef<str>) -> String {
    format!(
        "https://{}/{}/playlist.m3u8",
        pull_zone_host.as_ref().trim_end_matches('/'),
        video_id.as_ref()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embed_token_matches_bunny_formula() {
        // SHA256_HEX("mysecretkey" + "abc-123" + "1700000000") — verified with `sha256sum`.
        assert_eq!(
            embed_token("mysecretkey", "abc-123", 1_700_000_000),
            "54ce9c375a5def073eb128910d013c1f5ae5385fae501af7e807fe4f8de927f4"
        );
    }

    #[test]
    fn url_builders_shape() {
        assert_eq!(embed_url("12345", "abc-123"), "https://iframe.mediadelivery.net/embed/12345/abc-123");
        assert_eq!(
            embed_url_signed("12345", "abc-123", "mysecretkey", 1_700_000_000),
            "https://iframe.mediadelivery.net/embed/12345/abc-123?token=54ce9c375a5def073eb128910d013c1f5ae5385fae501af7e807fe4f8de927f4&expires=1700000000"
        );
        assert_eq!(hls_url("vz-abc.b-cdn.net", "abc-123"), "https://vz-abc.b-cdn.net/abc-123/playlist.m3u8");
        assert_eq!(hls_url("vz-abc.b-cdn.net/", "abc-123"), "https://vz-abc.b-cdn.net/abc-123/playlist.m3u8");
    }
}
