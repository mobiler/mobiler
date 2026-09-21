//! Labels for the built-in confirm dialog and date/time pickers, so an app can word the buttons in
//! its own language and say what they do ("Cancel booking" / "Keep it") instead of a generic
//! OK / Cancel. Used with [`Cx::confirm_with`](crate::Cx::confirm_with),
//! [`Cx::pick_date_with`](crate::Cx::pick_date_with) and
//! [`Cx::pick_time_with`](crate::Cx::pick_time_with).

use serde::Serialize;

/// A confirm dialog: title and message, optionally the two button labels, and whether the
/// confirming action is destructive (danger colour on Android and web, `.destructive` on iOS).
/// Unset labels keep the shell defaults (`OK` / `Cancel`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Confirm {
    title: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    confirm_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cancel_label: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    destructive: bool,
}

impl Confirm {
    #[must_use]
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self { title: title.into(), message: message.into(), confirm_label: None, cancel_label: None, destructive: false }
    }

    /// The confirming button — name the action ("Cancel booking"), not "OK".
    #[must_use]
    pub fn confirm_label(mut self, label: impl Into<String>) -> Self {
        self.confirm_label = Some(label.into());
        self
    }

    /// The dismissing button — the way out ("Keep it").
    #[must_use]
    pub fn cancel_label(mut self, label: impl Into<String>) -> Self {
        self.cancel_label = Some(label.into());
        self
    }

    /// The confirming action destroys something: the shells draw it in the danger style.
    #[must_use]
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    pub(crate) fn to_input(&self) -> String {
        serde_json::to_string(self).expect("serialize confirm")
    }
}

/// Labels for the native date/time picker; unset ones keep the shell defaults. The web shell opens
/// the browser's own picker, which draws its chrome in the browser's language and ignores these.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Picker {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    confirm_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cancel_label: Option<String>,
}

impl Picker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The picker's title — shown as the iOS action sheet title. Android sets it as the dialog
    /// title, but the Material date/time picker on current API levels doesn't display it; the web
    /// picker ignores it.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// The button that accepts the picked value.
    #[must_use]
    pub fn confirm_label(mut self, label: impl Into<String>) -> Self {
        self.confirm_label = Some(label.into());
        self
    }

    /// The button that dismisses the picker without a value.
    #[must_use]
    pub fn cancel_label(mut self, label: impl Into<String>) -> Self {
        self.cancel_label = Some(label.into());
        self
    }

    pub(crate) fn to_input(&self) -> String {
        serde_json::to_string(self).expect("serialize picker")
    }
}
