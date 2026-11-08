//! Protocole WebSocket entre le téléphone et le serveur.
//!
//! * Les messages de contrôle sont en JSON (`ClientMessage` / `ServerMessage`).
//! * Les flux à haute fréquence (capteurs, trackpad, défilement) sont des trames
//!   binaires compactes (`BinaryFrame`), en little-endian.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use std::collections::BTreeMap;

use crate::actions::{DeckView, InputValue};
use crate::settings::{Settings, VersionedSettings};

/// Version du protocole. Le serveur refuse un client d'une version majeure différente.
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Modifier {
    Ctrl,
    Shift,
    Alt,
    /// Touche Windows.
    Meta,
}

/// Touches spéciales. Le texte ordinaire passe par `ClientMessage::Text`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Key {
    Escape,
    Tab,
    Enter,
    Backspace,
    Delete,
    Space,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    PrintScreen,
    VolumeUp,
    VolumeDown,
    VolumeMute,
    MediaPlayPause,
    MediaNext,
    MediaPrevious,
    /// Une lettre ou un chiffre, pour les raccourcis (Ctrl+C…). Toujours en minuscule.
    #[serde(untagged)]
    Char(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MagnifierAction {
    Toggle,
    On,
    Off,
    ZoomIn,
    ZoomOut,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "t", rename_all = "snake_case")]
#[ts(export)]
pub enum ClientMessage {
    /// Premier message d'un appareil déjà appairé.
    Hello {
        token: String,
        protocol: u32,
    },
    /// Premier message d'un nouvel appareil, avec le code lu dans le QR.
    Pair {
        code: String,
        device_name: String,
        protocol: u32,
    },
    Button {
        button: MouseButton,
        down: bool,
    },
    /// Appui bref sur une touche, avec des modificateurs maintenus pendant l'appui.
    KeyTap {
        key: Key,
        modifiers: Vec<Modifier>,
    },
    Key {
        key: Key,
        down: bool,
    },
    Modifier {
        modifier: Modifier,
        down: bool,
    },
    /// Texte Unicode à taper tel quel (accents, emoji, autocorrection).
    Text {
        text: String,
    },
    /// Pouce posé sur la surface : active le pointage en mode `air_hold`.
    Clutch {
        engaged: bool,
    },
    /// Replace le curseur au centre de l'écran principal.
    Recenter,
    Magnifier {
        action: MagnifierAction,
    },
    UpdateSettings {
        #[ts(type = "number")]
        base_rev: u64,
        settings: Settings,
    },
    /// Lance un bouton du panneau d'actions, avec les valeurs saisies pour ses champs.
    RunAction {
        id: String,
        #[serde(default)]
        values: BTreeMap<String, InputValue>,
    },
    Ping {
        id: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ErrorCode {
    BadRequest,
    Unauthorized,
    PairingCodeInvalid,
    ProtocolMismatch,
    SettingsConflict,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "t", rename_all = "snake_case")]
#[ts(export)]
pub enum ServerMessage {
    /// Réponse à `Pair` : le jeton à conserver pour les connexions suivantes.
    Paired {
        device_id: String,
        token: String,
    },
    /// Session ouverte, avec l'état complet.
    Welcome {
        device_id: String,
        server_name: String,
        settings: VersionedSettings,
    },
    /// Les réglages ont changé (sur ce téléphone, un autre, ou le serveur).
    Settings {
        settings: VersionedSettings,
    },
    MagnifierState {
        active: bool,
        zoom: f32,
    },
    /// Le panneau d'actions (à l'ouverture de la session et à chaque modification sur le PC).
    Deck {
        deck: DeckView,
    },
    /// Fin d'une action lancée par ce téléphone.
    ActionResult {
        id: String,
        ok: bool,
        message: Option<String>,
    },
    Error {
        code: ErrorCode,
        message: String,
    },
    Pong {
        id: u32,
    },
}

/// Échantillon du gyroscope et de l'accéléromètre, dans le repère du téléphone.
///
/// Repère W3C : x vers la droite de l'écran, y vers le haut de l'écran,
/// z sortant de l'écran.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MotionSample {
    /// Vitesse angulaire autour de x, y, z en degrés par seconde.
    pub gyro_dps: [f32; 3],
    /// Accélération incluant la gravité, en m/s² (au repos, pointe vers le haut).
    pub accel: [f32; 3],
    /// Intervalle depuis l'échantillon précédent, en secondes.
    pub dt: f32,
}

/// Accélération linéaire du téléphone (gravité retirée), dans son repère, pour l'option
/// « embardée / pilonnement ».
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LinearSample {
    /// En m/s².
    pub accel: [f32; 3],
    /// Intervalle depuis l'échantillon précédent, en secondes.
    pub dt: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryFrame {
    Motion(MotionSample),
    Linear(LinearSample),
    /// Déplacement du doigt en pixels CSS.
    Trackpad {
        dx: f32,
        dy: f32,
    },
    /// Défilement à deux doigts en pixels CSS.
    Scroll {
        dx: f32,
        dy: f32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error("trame vide")]
    Empty,
    #[error("type de trame inconnu : {0:#04x}")]
    UnknownKind(u8),
    #[error("longueur invalide pour la trame {kind:#04x} : {len} octets")]
    BadLength { kind: u8, len: usize },
    #[error("valeur non finie dans la trame")]
    NonFinite,
}

const KIND_MOTION: u8 = 0x01;
const KIND_TRACKPAD: u8 = 0x02;
const KIND_SCROLL: u8 = 0x03;
const KIND_LINEAR: u8 = 0x04;

impl BinaryFrame {
    pub fn encode(&self) -> Vec<u8> {
        let (kind, values): (u8, Vec<f32>) = match self {
            BinaryFrame::Motion(m) => (
                KIND_MOTION,
                vec![
                    m.gyro_dps[0],
                    m.gyro_dps[1],
                    m.gyro_dps[2],
                    m.accel[0],
                    m.accel[1],
                    m.accel[2],
                    m.dt,
                ],
            ),
            BinaryFrame::Linear(l) => (KIND_LINEAR, vec![l.accel[0], l.accel[1], l.accel[2], l.dt]),
            BinaryFrame::Trackpad { dx, dy } => (KIND_TRACKPAD, vec![*dx, *dy]),
            BinaryFrame::Scroll { dx, dy } => (KIND_SCROLL, vec![*dx, *dy]),
        };
        let mut out = Vec::with_capacity(1 + values.len() * 4);
        out.push(kind);
        for v in values {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, FrameError> {
        let (&kind, payload) = bytes.split_first().ok_or(FrameError::Empty)?;
        let expected = match kind {
            KIND_MOTION => 7,
            KIND_LINEAR => 4,
            KIND_TRACKPAD | KIND_SCROLL => 2,
            other => return Err(FrameError::UnknownKind(other)),
        };
        if payload.len() != expected * 4 {
            return Err(FrameError::BadLength {
                kind,
                len: bytes.len(),
            });
        }
        let f: Vec<f32> = payload
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect();
        if f.iter().any(|v| !v.is_finite()) {
            return Err(FrameError::NonFinite);
        }
        Ok(match kind {
            KIND_MOTION => BinaryFrame::Motion(MotionSample {
                gyro_dps: [f[0], f[1], f[2]],
                accel: [f[3], f[4], f[5]],
                dt: f[6],
            }),
            KIND_LINEAR => BinaryFrame::Linear(LinearSample {
                accel: [f[0], f[1], f[2]],
                dt: f[3],
            }),
            KIND_TRACKPAD => BinaryFrame::Trackpad { dx: f[0], dy: f[1] },
            _ => BinaryFrame::Scroll { dx: f[0], dy: f[1] },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn linear_frames_round_trip() {
        let f = BinaryFrame::Linear(LinearSample {
            accel: [0.5, -1.0, 2.0],
            dt: 0.016,
        });
        assert_eq!(f.encode().len(), 17);
        assert_eq!(BinaryFrame::decode(&f.encode()), Ok(f));
    }

    #[test]
    fn client_messages_are_tagged_snake_case_json() {
        let msg = ClientMessage::KeyTap {
            key: Key::Char('c'),
            modifiers: vec![Modifier::Ctrl],
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"t": "key_tap", "key": "c", "modifiers": ["ctrl"]})
        );
    }

    #[test]
    fn named_keys_and_chars_round_trip() {
        for key in [
            Key::Escape,
            Key::ArrowLeft,
            Key::F12,
            Key::Char('a'),
            Key::Char('7'),
        ] {
            let json = serde_json::to_string(&key).unwrap();
            assert_eq!(serde_json::from_str::<Key>(&json).unwrap(), key, "{json}");
        }
        assert_eq!(
            serde_json::to_string(&Key::PageDown).unwrap(),
            "\"page_down\""
        );
    }

    #[test]
    fn parses_a_run_action_message() {
        let raw = r#"{"t":"run_action","id":"slower","values":{"speed":1.5,"site":"yt"}}"#;
        match serde_json::from_str::<ClientMessage>(raw).unwrap() {
            ClientMessage::RunAction { id, values } => {
                assert_eq!(id, "slower");
                assert_eq!(values["speed"], InputValue::Number(1.5));
                assert_eq!(values["site"], InputValue::Text("yt".into()));
            }
            other => panic!("{other:?}"),
        }
        let bare: ClientMessage = serde_json::from_str(r#"{"t":"run_action","id":"x"}"#).unwrap();
        assert!(matches!(bare, ClientMessage::RunAction { values, .. } if values.is_empty()));
    }

    #[test]
    fn parses_a_pair_message_from_the_client() {
        let raw = r#"{"t":"pair","code":"abc","device_name":"iPhone de Yann","protocol":1}"#;
        let msg: ClientMessage = serde_json::from_str(raw).unwrap();
        assert_eq!(
            msg,
            ClientMessage::Pair {
                code: "abc".into(),
                device_name: "iPhone de Yann".into(),
                protocol: 1
            }
        );
    }

    #[test]
    fn unknown_message_type_is_an_error() {
        assert!(serde_json::from_str::<ClientMessage>(r#"{"t":"format_c"}"#).is_err());
    }

    #[test]
    fn server_error_serializes_with_code() {
        let msg = ServerMessage::Error {
            code: ErrorCode::Unauthorized,
            message: "x".into(),
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["t"], "error");
        assert_eq!(json["code"], "unauthorized");
    }

    #[test]
    fn motion_frame_has_fixed_size() {
        let f = BinaryFrame::Motion(MotionSample::default());
        assert_eq!(f.encode().len(), 29);
        assert_eq!(BinaryFrame::Trackpad { dx: 1.0, dy: 2.0 }.encode().len(), 9);
    }

    #[test]
    fn decode_rejects_bad_frames() {
        assert_eq!(BinaryFrame::decode(&[]), Err(FrameError::Empty));
        assert_eq!(
            BinaryFrame::decode(&[0x7f]),
            Err(FrameError::UnknownKind(0x7f))
        );
        assert_eq!(
            BinaryFrame::decode(&[KIND_TRACKPAD, 0, 0]),
            Err(FrameError::BadLength {
                kind: KIND_TRACKPAD,
                len: 3
            })
        );
        let mut nan = BinaryFrame::Trackpad { dx: 0.0, dy: 0.0 }.encode();
        nan[1..5].copy_from_slice(&f32::NAN.to_le_bytes());
        assert_eq!(BinaryFrame::decode(&nan), Err(FrameError::NonFinite));
    }

    fn finite() -> impl Strategy<Value = f32> {
        -1.0e6f32..1.0e6f32
    }

    proptest! {
        #[test]
        fn motion_frames_round_trip(g in prop::array::uniform3(finite()), a in prop::array::uniform3(finite()), dt in 0.0f32..1.0) {
            let frame = BinaryFrame::Motion(MotionSample { gyro_dps: g, accel: a, dt });
            prop_assert_eq!(BinaryFrame::decode(&frame.encode()).unwrap(), frame);
        }

        #[test]
        fn trackpad_and_scroll_round_trip(dx in finite(), dy in finite()) {
            for frame in [BinaryFrame::Trackpad { dx, dy }, BinaryFrame::Scroll { dx, dy }] {
                prop_assert_eq!(BinaryFrame::decode(&frame.encode()).unwrap(), frame);
            }
        }

        #[test]
        fn decode_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..64)) {
            let _ = BinaryFrame::decode(&bytes);
        }
    }
}
