use crux_core::{
    Core,
    bridge::{Bridge, EffectId},
};

use crate::{Counter, Event, app::snapshot_buffer};

#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
pub struct CoreFFI {
    core: Bridge<Counter>,
}

impl Default for CoreFFI {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl CoreFFI {
    #[cfg_attr(feature = "uniffi", uniffi::constructor)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            core: Bridge::new(Core::new()),
        }
    }

    /// Send an event to the app and return the effects.
    /// # Panics
    /// If the event cannot be deserialized.
    #[must_use]
    pub fn update(&self, data: &[u8]) -> Vec<u8> {
        let mut effects = Vec::new();
        match self.core.update(data, &mut effects) {
            Ok(()) => effects,
            Err(e) => panic!("{e}"),
        }
    }

    /// Resolve an effect and return the effects.
    /// # Panics
    /// If the `data` cannot be deserialized into an effect or the `effect_id` is invalid.
    #[must_use]
    pub fn resolve(&self, id: u32, data: &[u8]) -> Vec<u8> {
        let mut effects = Vec::new();
        match self.core.resolve(EffectId(id), data, &mut effects) {
            Ok(()) => effects,
            Err(e) => panic!("{e}"),
        }
    }

    /// Get the current `ViewModel`.
    /// # Panics
    /// If the view cannot be serialized.
    #[must_use]
    pub fn view(&self) -> Vec<u8> {
        let mut view_model = Vec::new();
        match self.core.view(&mut view_model) {
            Ok(()) => view_model,
            Err(e) => panic!("{e}"),
        }
    }

    /// Bincode-serialized snapshot of the current Model.
    /// The shell persists this and feeds it back via `import_state` on relaunch.
    /// Returns an empty Vec if no update has run yet (e.g. fresh app, before any event).
    #[must_use]
    pub fn export_state(&self) -> Vec<u8> {
        snapshot_buffer().lock().unwrap().clone()
    }

    /// Restore Model from a previously-exported snapshot. Returns false on schema
    /// mismatch (e.g. saved state from an older app version); shell should treat
    /// that as "no saved state" and continue with the default Model.
    pub fn import_state(&self, data: &[u8]) -> bool {
        // Bridge owns the model, so we can't replace it directly. Route through
        // a synthetic Event::LoadFromSnapshot that the Counter::update handler knows
        // how to apply.
        let event = Event::LoadFromSnapshot(data.to_vec());
        let Ok(event_bytes) = bincode::serialize(&event) else {
            return false;
        };
        let mut effects = Vec::new();
        self.core.update(&event_bytes, &mut effects).is_ok()
    }
}
