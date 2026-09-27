//! The app's own version/build, handed to the core by the shell at startup
//! ([`Action::AppInfo`](mobiler_ui::Action::AppInfo)) and read via [`Cx::app_info`](crate::Cx::app_info).

use std::sync::RwLock;

/// The running app's version as the platform knows it — what the store compares on upload.
/// All fields are empty when the shell predates this (it never sent `Action::AppInfo`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppInfo {
    /// iOS `CFBundleShortVersionString` / Android `versionName` (e.g. `"1.0"`).
    pub version: String,
    /// iOS `CFBundleVersion` / Android `longVersionCode`, as text (iOS allows `"1.2.3"`).
    pub build: String,
    /// `"ios"`, `"android"` or `"web"`.
    pub platform: String,
    /// iOS bundle identifier / Android package name.
    pub bundle_id: String,
}

impl AppInfo {
    const EMPTY: AppInfo =
        AppInfo { version: String::new(), build: String::new(), platform: String::new(), bundle_id: String::new() };
}

// One core per process on every shell, and `MobilerShell` is stateless, so the value lives here.
static APP_INFO: RwLock<AppInfo> = RwLock::new(AppInfo::EMPTY);

/// Replace the stored info (the last `Action::AppInfo` wins, so a hot reload is harmless).
pub(crate) fn set(info: AppInfo) {
    *APP_INFO.write().unwrap_or_else(|e| e.into_inner()) = info;
}

/// A copy of the stored info.
pub(crate) fn get() -> AppInfo {
    APP_INFO.read().unwrap_or_else(|e| e.into_inner()).clone()
}
