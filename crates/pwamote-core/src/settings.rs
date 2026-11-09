//! Réglages partagés entre le serveur et les téléphones.
//!
//! Le serveur est la source de vérité. Chaque modification incrémente une
//! révision ; un client qui envoie des réglages basés sur une révision
//! périmée reçoit un conflit avec l'état courant (concurrence optimiste).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Comment le curseur réagit aux mouvements du téléphone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PointerMode {
    /// Pointage gyroscopique permanent (façon Wiimote).
    AirAlways,
    /// Pointage gyroscopique seulement tant que le pouce est posé.
    AirHold,
    /// Pas de gyroscope : l'écran sert de trackpad.
    Trackpad,
}

/// Ce que la loupe suit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MagnifierFollow {
    Cursor,
    Caret,
    /// Le caret pendant la frappe, le curseur sinon.
    Smart,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct PointerSettings {
    pub mode: PointerMode,
    /// Pixels par degré de rotation à vitesse lente.
    pub sensitivity: f32,
    /// 0 = linéaire, 1 = forte accélération pour les gestes rapides, jusqu'à 3 pour traverser deux écrans.
    pub acceleration: f32,
    /// 0 = brut, 1 = très lissé (filtre One Euro).
    pub smoothing: f32,
    /// Vitesse angulaire (°/s) en dessous de laquelle le tremblement est absorbé.
    pub deadzone_dps: f32,
    pub invert_y: bool,
    /// Inverse le sens gauche/droite (utile si le capteur du navigateur a ses axes inversés).
    pub invert_x: bool,
    /// Multiplicateur du mode trackpad.
    pub trackpad_speed: f32,
    pub scroll_speed: f32,
    pub natural_scroll: bool,
    /// Le déplacement du téléphone (gauche/droite, haut/bas) s'ajoute au lacet et au tangage.
    pub translation: bool,
    /// Pixels de curseur par mètre de déplacement du téléphone.
    pub translation_gain: f32,
}

impl Default for PointerSettings {
    fn default() -> Self {
        Self {
            mode: PointerMode::AirHold,
            sensitivity: 38.0,
            acceleration: 0.45,
            smoothing: 0.35,
            deadzone_dps: 1.6,
            invert_y: false,
            invert_x: false,
            trackpad_speed: 3.0,
            scroll_speed: 1.0,
            natural_scroll: true,
            translation: false,
            translation_gain: 2500.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct MagnifierSettings {
    /// Facteur de grossissement quand la loupe est active.
    pub zoom: f32,
    pub follow: MagnifierFollow,
    /// Durée de l'animation d'entrée/sortie, en millisecondes.
    pub animation_ms: u32,
    /// Délai après la dernière frappe pendant lequel le mode `smart` suit le caret.
    pub caret_hold_ms: u32,
}

impl Default for MagnifierSettings {
    fn default() -> Self {
        Self {
            zoom: 2.5,
            follow: MagnifierFollow::Smart,
            animation_ms: 220,
            caret_hold_ms: 1500,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct KeyboardSettings {
    /// Un appui sur un modificateur le verrouille pour la touche suivante.
    pub sticky_modifiers: bool,
}

impl Default for KeyboardSettings {
    fn default() -> Self {
        Self {
            sticky_modifiers: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct FeedbackSettings {
    pub haptics: bool,
}

impl Default for FeedbackSettings {
    fn default() -> Self {
        Self { haptics: true }
    }
}

/// Partage du presse-papiers entre le téléphone et le PC.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ClipboardSettings {
    /// Ce qui est copié sur le téléphone (texte ou image) peut être placé dans le presse-papiers du PC.
    /// Désactivé par défaut : le PC refuse alors tout envoi.
    pub to_pc: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Settings {
    pub pointer: PointerSettings,
    pub magnifier: MagnifierSettings,
    pub keyboard: KeyboardSettings,
    pub feedback: FeedbackSettings,
    pub clipboard: ClipboardSettings,
}

fn clamp_finite(v: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v.clamp(min, max)
    } else {
        fallback
    }
}

impl Settings {
    /// Ramène chaque valeur dans une plage sûre. Les valeurs non finies
    /// reprennent leur valeur par défaut.
    pub fn sanitized(mut self) -> Self {
        let d = Settings::default();
        let p = &mut self.pointer;
        p.sensitivity = clamp_finite(p.sensitivity, 5.0, 500.0, d.pointer.sensitivity);
        p.acceleration = clamp_finite(p.acceleration, 0.0, 3.0, d.pointer.acceleration);
        p.smoothing = clamp_finite(p.smoothing, 0.0, 1.0, d.pointer.smoothing);
        p.deadzone_dps = clamp_finite(p.deadzone_dps, 0.0, 10.0, d.pointer.deadzone_dps);
        p.trackpad_speed = clamp_finite(p.trackpad_speed, 0.2, 12.0, d.pointer.trackpad_speed);
        p.scroll_speed = clamp_finite(p.scroll_speed, 0.1, 6.0, d.pointer.scroll_speed);
        p.translation_gain = clamp_finite(
            p.translation_gain,
            100.0,
            20_000.0,
            d.pointer.translation_gain,
        );
        let m = &mut self.magnifier;
        m.zoom = clamp_finite(m.zoom, 1.25, 8.0, d.magnifier.zoom);
        m.animation_ms = m.animation_ms.min(1000);
        m.caret_hold_ms = m.caret_hold_ms.clamp(200, 10_000);
        self
    }
}

/// Réglages accompagnés de leur révision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VersionedSettings {
    #[ts(type = "number")]
    pub rev: u64,
    pub settings: Settings,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("révision périmée : le client était sur {client_rev}, le serveur est sur {}", current.rev)]
pub struct Conflict {
    pub client_rev: u64,
    pub current: VersionedSettings,
}

/// Magasin de réglages en mémoire, avec concurrence optimiste.
#[derive(Debug, Clone)]
pub struct SettingsStore {
    current: VersionedSettings,
}

impl SettingsStore {
    pub fn new(rev: u64, settings: Settings) -> Self {
        Self {
            current: VersionedSettings {
                rev,
                settings: settings.sanitized(),
            },
        }
    }

    pub fn current(&self) -> &VersionedSettings {
        &self.current
    }

    /// Applique `settings` si `base_rev` est la révision courante.
    /// Une mise à jour identique ne crée pas de nouvelle révision.
    pub fn update(
        &mut self,
        base_rev: u64,
        settings: Settings,
    ) -> Result<&VersionedSettings, Conflict> {
        if base_rev != self.current.rev {
            return Err(Conflict {
                client_rev: base_rev,
                current: self.current.clone(),
            });
        }
        let settings = settings.sanitized();
        if settings != self.current.settings {
            self.current = VersionedSettings {
                rev: self.current.rev + 1,
                settings,
            };
        }
        Ok(&self.current)
    }
}

impl Default for SettingsStore {
    fn default() -> Self {
        Self::new(0, Settings::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_already_sane() {
        let d = Settings::default();
        assert_eq!(d.clone().sanitized(), d);
    }

    #[test]
    fn sanitize_clamps_out_of_range_values() {
        let mut s = Settings::default();
        s.pointer.sensitivity = 10_000.0;
        s.pointer.smoothing = -3.0;
        s.magnifier.zoom = 0.5;
        let s = s.sanitized();
        assert_eq!(s.pointer.sensitivity, 500.0);
        assert_eq!(s.pointer.smoothing, 0.0);
        assert_eq!(s.magnifier.zoom, 1.25);
    }

    #[test]
    fn trackpad_speed_can_go_high_enough_to_cross_a_screen_in_one_swipe() {
        let mut s = Settings::default();
        s.pointer.trackpad_speed = 12.0;
        assert_eq!(s.sanitized().pointer.trackpad_speed, 12.0);
    }

    #[test]
    fn sanitize_replaces_non_finite_values_by_defaults() {
        let mut s = Settings::default();
        s.pointer.acceleration = f32::NAN;
        s.pointer.sensitivity = f32::INFINITY;
        let s = s.sanitized();
        assert_eq!(
            s.pointer.acceleration,
            PointerSettings::default().acceleration
        );
        assert_eq!(
            s.pointer.sensitivity,
            PointerSettings::default().sensitivity
        );
    }

    #[test]
    fn update_on_current_rev_bumps_revision() {
        let mut store = SettingsStore::default();
        let mut s = Settings::default();
        s.pointer.invert_y = true;
        let v = store.update(0, s.clone()).unwrap();
        assert_eq!(v.rev, 1);
        assert_eq!(v.settings, s);
    }

    #[test]
    fn identical_update_keeps_revision() {
        let mut store = SettingsStore::default();
        let v = store.update(0, Settings::default()).unwrap();
        assert_eq!(v.rev, 0);
    }

    #[test]
    fn stale_update_is_rejected_with_current_state() {
        let mut store = SettingsStore::default();
        let mut a = Settings::default();
        a.pointer.invert_y = true;
        store.update(0, a.clone()).unwrap();

        let mut b = Settings::default();
        b.pointer.sensitivity = 80.0;
        let err = store.update(0, b).unwrap_err();
        assert_eq!(err.client_rev, 0);
        assert_eq!(err.current.rev, 1);
        assert_eq!(err.current.settings, a);
    }

    #[test]
    fn update_is_sanitized() {
        let mut store = SettingsStore::default();
        let mut s = Settings::default();
        s.magnifier.zoom = 99.0;
        let v = store.update(0, s).unwrap();
        assert_eq!(v.settings.magnifier.zoom, 8.0);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults_when_deserializing() {
        // Un vieux fichier de réglages ne doit jamais casser le démarrage.
        let s: Settings = serde_json::from_str(r#"{"pointer":{"invert_y":true}}"#).unwrap();
        assert!(s.pointer.invert_y);
        assert_eq!(
            s.pointer.sensitivity,
            PointerSettings::default().sensitivity
        );
        assert_eq!(s.magnifier, MagnifierSettings::default());
    }

    #[test]
    fn clipboard_sharing_is_off_by_default_and_survives_old_files() {
        assert!(!Settings::default().clipboard.to_pc);
        let s: Settings = serde_json::from_str(r#"{"clipboard":{"to_pc":true}}"#).unwrap();
        assert!(s.clipboard.to_pc);
        let old: Settings = serde_json::from_str(r#"{"pointer":{}}"#).unwrap();
        assert!(!old.clipboard.to_pc);
    }

    #[test]
    fn json_uses_snake_case_enums() {
        let json = serde_json::to_value(Settings::default()).unwrap();
        assert_eq!(json["pointer"]["mode"], "air_hold");
        assert_eq!(json["magnifier"]["follow"], "smart");
    }
}
