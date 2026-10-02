//! Structured device details (`cx.device_info`), for support and bug reports.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::PluginResponse;

/// The phone's OS version and model, from the built-in `device` capability's `info` op. Read-only
/// and non-identifying: no serial, advertising id or user-set device name. A field the shell can't
/// know is empty (`0` for `os_api_level`); the web shell knows none of them.
#[derive(Facet, Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct DeviceInfo {
    /// `"13"` (Android `Build.VERSION.RELEASE`) / `"17.5"` (iOS `systemVersion`).
    pub os_version: String,
    /// Android `Build.VERSION.SDK_INT`; `0` elsewhere.
    pub os_api_level: u32,
    /// `"samsung"` / `"Apple"`.
    pub manufacturer: String,
    /// `"SM-A325F"` (Android `Build.MODEL`) / `"iPhone15,2"` (iOS hardware identifier).
    pub model: String,
}

impl DeviceInfo {
    /// Serialize for a `PluginResponse.output` (crux's FFI format, like [`crate::HttpOutcome`]).
    pub fn encode(&self) -> Vec<u8> {
        use crux_core::bridge::{BincodeFfiFormat, FfiFormat};
        let mut buffer = Vec::new();
        BincodeFfiFormat::serialize(&mut buffer, self).expect("encode DeviceInfo");
        buffer
    }

    /// Decode a shell's `device`/`info` payload.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        use crux_core::bridge::{BincodeFfiFormat, FfiFormat};
        BincodeFfiFormat::deserialize(bytes).map_err(|e| e.to_string())
    }
}

/// `Some` for a shell that answered; `None` when it refused (`ok: false`, e.g. a shell that
/// predates `info`) or sent something undecodable.
pub(crate) fn from_response(r: &PluginResponse) -> Option<DeviceInfo> {
    if !r.ok {
        return None;
    }
    DeviceInfo::decode(&r.output).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DeviceInfo {
        DeviceInfo { os_version: "13".into(), os_api_level: 33, manufacturer: "samsung".into(), model: "SM-A325F".into() }
    }

    #[test]
    fn device_info_round_trips() {
        assert_eq!(DeviceInfo::decode(&sample().encode()), Ok(sample()));
    }

    #[test]
    fn a_refused_or_garbled_reply_is_none() {
        assert_eq!(from_response(&PluginResponse { ok: true, output: sample().encode() }), Some(sample()));
        assert_eq!(from_response(&PluginResponse { ok: false, output: b"unknown op".to_vec() }), None);
        assert_eq!(from_response(&PluginResponse { ok: true, output: vec![] }), None);
    }
}
