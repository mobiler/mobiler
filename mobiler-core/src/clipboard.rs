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
/// - `expires_after`: iOS removes the item itself. On Android it is **best effort**: the shell clears the clipboard
///   when the time is up, but only while it still holds this copy. Android 10+ lets only the app in front read the
///   clipboard, so after the user switches away (say, to paste the secret into another app) it clears within about
///   5 s of the app coming back to the front, gives up an hour after the deadline, and never clears if the process dies first.
///   Android 13+ clears the clipboard itself after about an hour. The hidden preview is Android's dependable
///   protection. The web can't expire a copy.
/// - `local_only`: iOS keeps the item out of Universal Clipboard. No effect on Android (an OEM or companion-app
///   clipboard sync may still carry it) or the web.
///
/// A shell that predates `clipboard_with` answers `ok: false`. Don't fall back to `cx.copy` then: that is the plain
/// copy this exists to avoid.
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
    #[must_use]
    pub fn new() -> Self {
        Self { sensitive: false, expires_after: None, local_only: false }
    }

    /// Sensitive, expires after 2 minutes, kept off other devices (iOS). Every field can still be changed.
    #[must_use]
    pub fn sensitive() -> Self {
        Self { sensitive: true, expires_after: Some(Duration::from_secs(120)), local_only: true }
    }

    /// Hide the text in the clipboard preview and ask keyboards not to keep it (Android).
    #[must_use]
    pub fn mark_sensitive(mut self, on: bool) -> Self {
        self.sensitive = on;
        self
    }

    /// Remove the copy after `d` (at least 1 s); best effort on Android, see [`CopyOptions`].
    #[must_use]
    pub fn expires_after(mut self, d: Duration) -> Self {
        self.expires_after = Some(d.max(Duration::from_secs(1)));
        self
    }

    /// Leave the copy on the clipboard until something replaces it.
    #[must_use]
    pub fn never_expires(mut self) -> Self {
        self.expires_after = None;
        self
    }

    /// Keep the copy on this device (iOS: out of Universal Clipboard).
    #[must_use]
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

/// What a `clipboard_with` shell applied to a copy it made (ADR-0053), read from an `ok: true` reply with
/// [`CopyApplied::from_response`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CopyApplied {
    /// The copy carries the platform's sensitive marker: Android's `IS_SENSITIVE` extra (the preview hides it on 13+),
    /// or iOS's `org.nspasteboard.ConcealedType` convention. Never on the web.
    pub sensitive: bool,
    /// Who removes the copy when its time is up.
    pub expires: CopyExpiry,
    /// The platform keeps the copy off the user's other devices (iOS `.localOnly`). Never on Android or the web.
    pub local_only: bool,
}

/// How a copy expires, as the shell reported it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyExpiry {
    /// The OS removes it (iOS `.expirationDate`).
    System,
    /// The shell clears it if the clipboard still holds it (Android; ADR-0052).
    BestEffort,
    /// It doesn't expire: none was asked, or the platform can't (the web).
    None,
}

impl CopyApplied {
    /// The report in an `ok: true` reply, or `None` when there is none to trust: the copy failed, the output is
    /// empty (a shell from CLI 0.68 reports nothing), it doesn't parse, or it names an expiry this core doesn't know.
    /// `None` means *unknown*: an app that needs a protected copy treats it as not protected.
    pub fn from_response(resp: &crate::PluginResponse) -> Option<Self> {
        if !resp.ok {
            return None;
        }
        let v: serde_json::Value = serde_json::from_slice(&resp.output).ok()?;
        let expires = match v.get("expires")?.as_str()? {
            "system" => CopyExpiry::System,
            "best_effort" => CopyExpiry::BestEffort,
            "none" => CopyExpiry::None,
            _ => return None,
        };
        Some(Self { sensitive: v.get("sensitive")?.as_bool()?, expires, local_only: v.get("local_only")?.as_bool()? })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PluginResponse;

    fn ok(s: &str) -> PluginResponse {
        PluginResponse::text(true, s)
    }

    #[test]
    fn each_shells_sensitive_report_parses() {
        let android = ok(r#"{"sensitive":true,"expires":"best_effort","local_only":false}"#);
        let ios = ok(r#"{"sensitive":true,"expires":"system","local_only":true}"#);
        let web = ok(r#"{"sensitive":false,"expires":"none","local_only":false}"#);
        assert_eq!(CopyApplied::from_response(&android), Some(CopyApplied { sensitive: true, expires: CopyExpiry::BestEffort, local_only: false }));
        assert_eq!(CopyApplied::from_response(&ios), Some(CopyApplied { sensitive: true, expires: CopyExpiry::System, local_only: true }));
        assert_eq!(CopyApplied::from_response(&web), Some(CopyApplied { sensitive: false, expires: CopyExpiry::None, local_only: false }));
    }

    #[test]
    fn an_empty_report_is_unknown() {
        // A shell from CLI 0.68 answers ok:true with an empty output.
        assert_eq!(CopyApplied::from_response(&ok("")), None);
    }

    #[test]
    fn a_failed_copy_has_no_report() {
        let failed = PluginResponse::text(false, r#"{"sensitive":true,"expires":"system","local_only":true}"#);
        assert_eq!(CopyApplied::from_response(&failed), None);
    }

    #[test]
    fn an_unknown_expiry_is_unknown() {
        assert_eq!(CopyApplied::from_response(&ok(r#"{"sensitive":true,"expires":"later","local_only":true}"#)), None);
    }

    #[test]
    fn a_malformed_or_partial_report_is_unknown() {
        assert_eq!(CopyApplied::from_response(&ok("not json")), None);
        assert_eq!(CopyApplied::from_response(&ok(r#"{"sensitive":true,"expires":"system"}"#)), None);
        assert_eq!(CopyApplied::from_response(&ok(r#"{"sensitive":"yes","expires":"system","local_only":true}"#)), None);
    }
}
