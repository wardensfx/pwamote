//! Ports vers le système : injection de saisie et loupe.
//! Les implémentations Windows vivent dans `platform::windows`.

use pwamote_core::pointer::WheelDelta;
use pwamote_core::protocol::{Key, MagnifierAction, Modifier, MouseButton};
use pwamote_core::settings::MagnifierSettings;

pub trait InputSink: Send + Sync {
    fn move_by(&self, dx: i32, dy: i32);
    fn button(&self, button: MouseButton, down: bool);
    fn wheel(&self, delta: WheelDelta);
    fn key(&self, key: Key, down: bool);
    fn modifier(&self, modifier: Modifier, down: bool);
    fn text(&self, text: &str);
    /// Place le curseur au centre de l'écran principal.
    fn recenter(&self);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MagnifierStatus {
    pub active: bool,
    pub zoom: f32,
}

pub trait MagnifierSink: Send + Sync {
    fn command(&self, action: MagnifierAction) -> MagnifierStatus;
    fn configure(&self, settings: &MagnifierSettings);
}

/// Implémentation qui se contente de journaliser : utile hors Windows,
/// pour développer le client sur n'importe quelle machine.
pub struct LogInput;

impl InputSink for LogInput {
    fn move_by(&self, dx: i32, dy: i32) {
        tracing::trace!(dx, dy, "move");
    }
    fn button(&self, button: MouseButton, down: bool) {
        tracing::info!(?button, down, "button");
    }
    fn wheel(&self, delta: WheelDelta) {
        tracing::debug!(?delta, "wheel");
    }
    fn key(&self, key: Key, down: bool) {
        tracing::info!(?key, down, "key");
    }
    fn modifier(&self, modifier: Modifier, down: bool) {
        tracing::info!(?modifier, down, "modifier");
    }
    fn text(&self, text: &str) {
        tracing::info!(text, "text");
    }
    fn recenter(&self) {
        tracing::info!("recenter");
    }
}

/// Loupe sans effet, avec un état cohérent pour le client.
pub struct LogMagnifier {
    state: std::sync::Mutex<pwamote_core::magnifier::MagnifierController>,
}

impl LogMagnifier {
    pub fn new(settings: &MagnifierSettings) -> Self {
        Self {
            state: std::sync::Mutex::new(pwamote_core::magnifier::MagnifierController::new(
                settings,
            )),
        }
    }
}

impl MagnifierSink for LogMagnifier {
    fn command(&self, action: MagnifierAction) -> MagnifierStatus {
        let mut m = self.state.lock().expect("magnifier lock");
        apply_action(&mut m, action);
        tracing::info!(?action, active = m.is_active(), "magnifier");
        MagnifierStatus {
            active: m.is_active(),
            zoom: m.target_zoom(),
        }
    }
    fn configure(&self, settings: &MagnifierSettings) {
        self.state
            .lock()
            .expect("magnifier lock")
            .set_zoom(settings.zoom);
    }
}

pub fn apply_action(m: &mut pwamote_core::magnifier::MagnifierController, action: MagnifierAction) {
    match action {
        MagnifierAction::Toggle => m.toggle(),
        MagnifierAction::On => m.set_active(true),
        MagnifierAction::Off => m.set_active(false),
        MagnifierAction::ZoomIn => {
            m.zoom_by(1.25);
            m.set_active(true);
        }
        MagnifierAction::ZoomOut => m.zoom_by(0.8),
    }
}
