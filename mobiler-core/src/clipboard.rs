//! A copy with options (ADR-0052): `cx.copy_with` / `cx.copy_sensitive`. The request goes to the built-in
//! `clipboard_with` capability, not `clipboard`, so a shell that predates it answers `ok: false` instead of copying a
//! secret in plain text.

use std::time::Duration;

/// How [`Cx::copy_with`](crate::Cx::copy_with) copies. [`CopyOptions::new`] is a plain copy (like `cx.copy`, but with
/// a reply); [`CopyOptions::sensitive`] is the preset for a password, a recovery code or a TOTP secret.
///
/// What each platform honours:
/// - `mark_sensitive`: Android hides the text in the clipboard preview (Android 13+) and asks keyboards not to keep it.
///   iOS has no equivalent. The web can't.
/// - `expires_after`: iOS removes the item itself. Android clears the clipboard when the time is up, but only while it
///   still holds this copy; from the background it re-checks every 5 s until the app is in front, for up to an hour.
///   The web can't.
/// - `local_only`: iOS keeps the item out of Universal Clipboard. No effect on Android or the web.
#[derive(Clone, Debug, PartialEq)]
pub struct CopyOptions {
    sensitive: bool,
    expires_after: Option<Duration>,
    local_only: bool,
}

impl Default for CopyOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl CopyOptions {
    /// A plain copy: not sensitive, never expires, may sync to the user's other devices.
    pub fn new() -> Self {
        Self { sensitive: false, expires_after: None, local_only: false }
    }

    /// Sensitive, expires after 2 minutes, this device only. Every field can still be changed.
    pub fn sensitive() -> Self {
        Self { sensitive: true, expires_after: Some(Duration::from_secs(120)), local_only: true }
    }

    pub fn mark_sensitive(mut self, on: bool) -> Self {
        self.sensitive = on;
        self
    }

    /// Remove the copy after `d` (at least 1 s).
    pub fn expires_after(mut self, d: Duration) -> Self {
        self.expires_after = Some(d.max(Duration::from_secs(1)));
        self
    }

    pub fn never_expires(mut self) -> Self {
        self.expires_after = None;
        self
    }

    /// Keep the copy on this device (iOS: out of Universal Clipboard).
    pub fn local_only(mut self, on: bool) -> Self {
        self.local_only = on;
        self
    }

    /// The `clipboard_with` request's input: every key always present (ADR-0052).
    pub(crate) fn to_input(&self, text: &str) -> String {
        let expires_ms = self.expires_after.map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
        serde_json::json!({
            "text": text,
            "sensitive": self.sensitive,
            "expires_ms": expires_ms,
            "local_only": self.local_only,
        })
        .to_string()
    }
}
