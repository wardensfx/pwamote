//! Une session = un téléphone connecté. Machine à états indépendante du
//! transport : le WebSocket lui passe des messages, elle renvoie des réponses
//! et pilote les ports système.

use std::collections::HashSet;
use std::sync::Arc;

use pwamote_core::pointer::PointerEngine;
use pwamote_core::protocol::{
    BinaryFrame, ClientMessage, ErrorCode, Key, Modifier, MouseButton, ServerMessage,
    PROTOCOL_VERSION,
};

use crate::state::AppState;

#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    pub replies: Vec<ServerMessage>,
    /// La connexion doit être fermée après l'envoi des réponses.
    pub close: bool,
}

impl Outcome {
    fn reply(msg: ServerMessage) -> Self {
        Self {
            replies: vec![msg],
            close: false,
        }
    }

    fn none() -> Self {
        Self::default()
    }

    fn fatal(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            replies: vec![ServerMessage::Error {
                code,
                message: message.into(),
            }],
            close: true,
        }
    }
}

pub struct Session {
    state: Arc<AppState>,
    device_id: Option<String>,
    engine: PointerEngine,
    held_buttons: HashSet<MouseButton>,
    held_keys: HashSet<Key>,
    held_modifiers: HashSet<Modifier>,
}

impl Session {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            device_id: None,
            engine: PointerEngine::new(),
            held_buttons: HashSet::new(),
            held_keys: HashSet::new(),
            held_modifiers: HashSet::new(),
        }
    }

    pub fn device_id(&self) -> Option<&str> {
        self.device_id.as_deref()
    }

    fn welcome(&mut self, device_id: String) -> ServerMessage {
        self.device_id = Some(device_id.clone());
        self.state.connection_opened();
        ServerMessage::Welcome {
            device_id,
            server_name: self.state.server_name.clone(),
            settings: self.state.settings(),
        }
    }

    pub fn on_text(&mut self, raw: &str) -> Outcome {
        let msg: ClientMessage = match serde_json::from_str(raw) {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!(error = %e, "message JSON invalide");
                return if self.device_id.is_some() {
                    Outcome::reply(ServerMessage::Error {
                        code: ErrorCode::BadRequest,
                        message: e.to_string(),
                    })
                } else {
                    Outcome::fatal(ErrorCode::BadRequest, e.to_string())
                };
            }
        };

        if self.device_id.is_none() {
            return self.on_handshake(msg);
        }
        self.on_command(msg)
    }

    fn on_handshake(&mut self, msg: ClientMessage) -> Outcome {
        match msg {
            ClientMessage::Ping { id } => Outcome::reply(ServerMessage::Pong { id }),
            ClientMessage::Hello { protocol, .. } | ClientMessage::Pair { protocol, .. }
                if protocol != PROTOCOL_VERSION =>
            {
                Outcome::fatal(
                    ErrorCode::ProtocolMismatch,
                    format!("le serveur parle la version {PROTOCOL_VERSION}, le client la version {protocol}"),
                )
            }
            ClientMessage::Hello { token, .. } => match self.state.authenticate(&token) {
                Some(device) => {
                    tracing::info!(device = %device.name, "téléphone connecté");
                    Outcome::reply(self.welcome(device.id))
                }
                None => Outcome::fatal(ErrorCode::Unauthorized, "appareil inconnu, appairez-le de nouveau"),
            },
            ClientMessage::Pair { code, device_name, .. } => match self.state.pair(&code, &device_name) {
                Ok((device_id, token)) => {
                    tracing::info!(device = %device_name, "nouvel appareil appairé");
                    let paired = ServerMessage::Paired { device_id: device_id.clone(), token };
                    let welcome = self.welcome(device_id);
                    Outcome { replies: vec![paired, welcome], close: false }
                }
                Err(e) => Outcome::fatal(ErrorCode::PairingCodeInvalid, e.to_string()),
            },
            _ => Outcome::fatal(ErrorCode::Unauthorized, "authentification requise"),
        }
    }

    fn on_command(&mut self, msg: ClientMessage) -> Outcome {
        let input = self.state.input.clone();
        match msg {
            ClientMessage::Hello { .. } | ClientMessage::Pair { .. } => {
                return Outcome::reply(ServerMessage::Error {
                    code: ErrorCode::BadRequest,
                    message: "session déjà ouverte".into(),
                })
            }
            ClientMessage::Ping { id } => return Outcome::reply(ServerMessage::Pong { id }),
            ClientMessage::Button { button, down } => {
                // On n'émet que les vraies transitions : un double « down » est ignoré.
                let changed = if down {
                    self.held_buttons.insert(button)
                } else {
                    self.held_buttons.remove(&button)
                };
                if changed {
                    input.button(button, down);
                }
            }
            ClientMessage::Key { key, down } => {
                let changed = if down {
                    self.held_keys.insert(key)
                } else {
                    self.held_keys.remove(&key)
                };
                if changed {
                    input.key(key, down);
                }
            }
            ClientMessage::Modifier { modifier, down } => {
                let changed = if down {
                    self.held_modifiers.insert(modifier)
                } else {
                    self.held_modifiers.remove(&modifier)
                };
                if changed {
                    input.modifier(modifier, down);
                }
            }
            ClientMessage::KeyTap { key, modifiers } => {
                let extra: Vec<Modifier> = dedup(&modifiers)
                    .into_iter()
                    .filter(|m| !self.held_modifiers.contains(m))
                    .collect();
                for m in &extra {
                    input.modifier(*m, true);
                }
                input.key(key, true);
                input.key(key, false);
                for m in extra.iter().rev() {
                    input.modifier(*m, false);
                }
            }
            ClientMessage::Text { text } => {
                if !text.is_empty() {
                    input.text(&text);
                }
            }
            ClientMessage::Clutch { engaged } => self.engine.set_clutch(engaged),
            ClientMessage::Recenter => input.recenter(),
            ClientMessage::Magnifier { action } => {
                let status = self.state.magnifier.command(action);
                return Outcome::reply(ServerMessage::MagnifierState {
                    active: status.active,
                    zoom: status.zoom,
                });
            }
            ClientMessage::UpdateSettings { base_rev, settings } => {
                return match self.state.update_settings(base_rev, settings) {
                    Ok(settings) => Outcome::reply(ServerMessage::Settings { settings }),
                    Err(conflict) => Outcome {
                        replies: vec![
                            ServerMessage::Error {
                                code: ErrorCode::SettingsConflict,
                                message: conflict.to_string(),
                            },
                            ServerMessage::Settings {
                                settings: conflict.current,
                            },
                        ],
                        close: false,
                    },
                };
            }
        }
        Outcome::none()
    }

    pub fn on_binary(&mut self, bytes: &[u8]) {
        if self.device_id.is_none() {
            return;
        }
        let frame = match BinaryFrame::decode(bytes) {
            Ok(f) => f,
            Err(e) => {
                tracing::debug!(error = %e, "trame binaire ignorée");
                return;
            }
        };
        let cfg = self.state.settings().settings.pointer;
        let input = &self.state.input;
        match frame {
            BinaryFrame::Motion(sample) => {
                let d = self.engine.on_motion(&sample, &cfg);
                if !d.is_zero() {
                    input.move_by(d.dx, d.dy);
                }
            }
            BinaryFrame::Linear(sample) => {
                let d = self.engine.on_linear(&sample, &cfg);
                if !d.is_zero() {
                    input.move_by(d.dx, d.dy);
                }
            }
            BinaryFrame::Trackpad { dx, dy } => {
                let d = self.engine.on_trackpad(dx, dy, &cfg);
                if !d.is_zero() {
                    input.move_by(d.dx, d.dy);
                }
            }
            BinaryFrame::Scroll { dx, dy } => {
                let w = self.engine.on_scroll(dx, dy, &cfg);
                if w.horizontal != 0 || w.vertical != 0 {
                    input.wheel(w);
                }
            }
        }
    }

    /// Relâche tout ce qui est maintenu. Appelé à la déconnexion, pour ne
    /// jamais laisser un clic ou une touche Ctrl collés sur le PC.
    pub fn release_all(&mut self) {
        let input = &self.state.input;
        for b in self.held_buttons.drain() {
            input.button(b, false);
        }
        for k in self.held_keys.drain() {
            input.key(k, false);
        }
        for m in self.held_modifiers.drain() {
            input.modifier(m, false);
        }
        self.engine.set_clutch(false);
    }
}

fn dedup(modifiers: &[Modifier]) -> Vec<Modifier> {
    let mut out = Vec::new();
    for m in modifiers {
        if !out.contains(m) {
            out.push(*m);
        }
    }
    out
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.device_id.is_some() {
            self.state.connection_closed();
        }
        self.release_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::{fixture, Event, Fixture};
    use pwamote_core::protocol::{MagnifierAction, MotionSample};
    use pwamote_core::settings::{PointerMode, Settings};
    use serde_json::json;

    fn send(s: &mut Session, v: serde_json::Value) -> Outcome {
        s.on_text(&v.to_string())
    }

    fn paired_session() -> (Fixture, Session) {
        let f = fixture();
        let mut s = Session::new(f.state.clone());
        let code = f.state.new_pairing_code();
        let out = send(
            &mut s,
            json!({"t": "pair", "code": code, "device_name": "iPhone", "protocol": 1}),
        );
        assert!(!out.close, "{out:?}");
        (f, s)
    }

    #[test]
    fn pairing_returns_a_token_then_welcome() {
        let f = fixture();
        let mut s = Session::new(f.state.clone());
        let code = f.state.new_pairing_code();
        let out = send(
            &mut s,
            json!({"t": "pair", "code": code, "device_name": "iPhone", "protocol": 1}),
        );
        assert!(matches!(out.replies[0], ServerMessage::Paired { .. }));
        assert!(matches!(out.replies[1], ServerMessage::Welcome { .. }));
        assert!(s.device_id().is_some());
    }

    #[test]
    fn hello_with_a_known_token_opens_the_session() {
        let f = fixture();
        let code = f.state.new_pairing_code();
        let (_, token) = f.state.pair(&code, "iPhone").unwrap();
        let mut s = Session::new(f.state.clone());
        let out = send(&mut s, json!({"t": "hello", "token": token, "protocol": 1}));
        match &out.replies[0] {
            ServerMessage::Welcome {
                server_name,
                settings,
                ..
            } => {
                assert_eq!(server_name, "Test PC");
                assert_eq!(settings.rev, 0);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unknown_token_is_rejected_and_closed() {
        let f = fixture();
        let mut s = Session::new(f.state.clone());
        let out = send(
            &mut s,
            json!({"t": "hello", "token": "nope", "protocol": 1}),
        );
        assert!(out.close);
        assert!(matches!(
            out.replies[0],
            ServerMessage::Error {
                code: ErrorCode::Unauthorized,
                ..
            }
        ));
    }

    #[test]
    fn wrong_protocol_version_is_rejected() {
        let f = fixture();
        let mut s = Session::new(f.state.clone());
        let out = send(&mut s, json!({"t": "hello", "token": "x", "protocol": 99}));
        assert!(out.close);
        assert!(matches!(
            out.replies[0],
            ServerMessage::Error {
                code: ErrorCode::ProtocolMismatch,
                ..
            }
        ));
    }

    #[test]
    fn commands_before_authentication_are_refused() {
        let f = fixture();
        let mut s = Session::new(f.state.clone());
        let out = send(
            &mut s,
            json!({"t": "button", "button": "left", "down": true}),
        );
        assert!(out.close);
        assert!(f.input.take().is_empty());
        s.on_binary(&BinaryFrame::Trackpad { dx: 50.0, dy: 0.0 }.encode());
        assert!(f.input.take().is_empty());
    }

    #[test]
    fn a_bad_pairing_code_is_refused() {
        let f = fixture();
        let mut s = Session::new(f.state.clone());
        let out = send(
            &mut s,
            json!({"t": "pair", "code": "forged", "device_name": "x", "protocol": 1}),
        );
        assert!(out.close);
        assert!(matches!(
            out.replies[0],
            ServerMessage::Error {
                code: ErrorCode::PairingCodeInvalid,
                ..
            }
        ));
    }

    #[test]
    fn buttons_only_emit_real_transitions() {
        let (f, mut s) = paired_session();
        send(
            &mut s,
            json!({"t": "button", "button": "left", "down": true}),
        );
        send(
            &mut s,
            json!({"t": "button", "button": "left", "down": true}),
        );
        send(
            &mut s,
            json!({"t": "button", "button": "left", "down": false}),
        );
        assert_eq!(
            f.input.take(),
            vec![
                Event::Button(MouseButton::Left, true),
                Event::Button(MouseButton::Left, false)
            ]
        );
    }

    #[test]
    fn key_tap_wraps_the_key_with_its_modifiers() {
        let (f, mut s) = paired_session();
        send(
            &mut s,
            json!({"t": "key_tap", "key": "c", "modifiers": ["ctrl", "shift", "ctrl"]}),
        );
        assert_eq!(
            f.input.take(),
            vec![
                Event::Modifier(Modifier::Ctrl, true),
                Event::Modifier(Modifier::Shift, true),
                Event::Key(Key::Char('c'), true),
                Event::Key(Key::Char('c'), false),
                Event::Modifier(Modifier::Shift, false),
                Event::Modifier(Modifier::Ctrl, false),
            ]
        );
    }

    #[test]
    fn key_tap_does_not_release_a_modifier_held_by_the_user() {
        let (f, mut s) = paired_session();
        send(
            &mut s,
            json!({"t": "modifier", "modifier": "alt", "down": true}),
        );
        send(
            &mut s,
            json!({"t": "key_tap", "key": "tab", "modifiers": ["alt"]}),
        );
        assert_eq!(
            f.input.take(),
            vec![
                Event::Modifier(Modifier::Alt, true),
                Event::Key(Key::Tab, true),
                Event::Key(Key::Tab, false),
            ]
        );
    }

    #[test]
    fn text_is_forwarded_as_unicode() {
        let (f, mut s) = paired_session();
        send(&mut s, json!({"t": "text", "text": "Ça marche 👌"}));
        send(&mut s, json!({"t": "text", "text": ""}));
        assert_eq!(f.input.take(), vec![Event::Text("Ça marche 👌".into())]);
    }

    #[test]
    fn disconnect_releases_everything_held() {
        let (f, mut s) = paired_session();
        send(
            &mut s,
            json!({"t": "button", "button": "left", "down": true}),
        );
        send(
            &mut s,
            json!({"t": "modifier", "modifier": "ctrl", "down": true}),
        );
        f.input.take();
        drop(s);
        let mut events = f.input.take();
        events.sort_by_key(|e| format!("{e:?}"));
        assert_eq!(
            events,
            vec![
                Event::Button(MouseButton::Left, false),
                Event::Modifier(Modifier::Ctrl, false)
            ]
        );
    }

    #[test]
    fn gyro_frames_move_the_cursor_in_air_mode() {
        let (f, mut s) = paired_session();
        let mut settings = Settings::default();
        settings.pointer.mode = PointerMode::AirAlways;
        f.state.update_settings(0, settings).unwrap();
        for _ in 0..20 {
            let frame = BinaryFrame::Motion(MotionSample {
                gyro_dps: [0.0, 0.0, 60.0],
                accel: [0.0, 0.0, 9.81],
                dt: 1.0 / 60.0,
            });
            s.on_binary(&frame.encode());
        }
        let dx: i32 = f
            .input
            .take()
            .iter()
            .map(|e| if let Event::Move(dx, _) = e { *dx } else { 0 })
            .sum();
        assert!(dx < -100, "dx = {dx}");
    }

    #[test]
    fn hold_mode_needs_the_clutch() {
        let (f, mut s) = paired_session();
        let frame = BinaryFrame::Motion(MotionSample {
            gyro_dps: [0.0, 0.0, 60.0],
            accel: [0.0, 0.0, 9.81],
            dt: 1.0 / 60.0,
        })
        .encode();
        s.on_binary(&frame);
        assert!(f.input.take().is_empty(), "air_hold est le mode par défaut");
        send(&mut s, json!({"t": "clutch", "engaged": true}));
        for _ in 0..10 {
            s.on_binary(&frame);
        }
        assert!(!f.input.take().is_empty());
    }

    #[test]
    fn scroll_frames_turn_the_wheel() {
        let (f, mut s) = paired_session();
        s.on_binary(&BinaryFrame::Scroll { dx: 0.0, dy: 40.0 }.encode());
        assert!(matches!(f.input.take()[..], [Event::Wheel(w)] if w.vertical > 0));
    }

    #[test]
    fn settings_conflict_returns_the_current_state() {
        let (f, mut s) = paired_session();
        let mut a = Settings::default();
        a.pointer.invert_y = true;
        f.state.update_settings(0, a.clone()).unwrap();
        let out = send(
            &mut s,
            json!({"t": "update_settings", "base_rev": 0, "settings": serde_json::to_value(Settings::default()).unwrap()}),
        );
        assert!(matches!(
            out.replies[0],
            ServerMessage::Error {
                code: ErrorCode::SettingsConflict,
                ..
            }
        ));
        match &out.replies[1] {
            ServerMessage::Settings { settings } => assert_eq!(settings.settings, a),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn magnifier_commands_report_their_state() {
        let (_f, mut s) = paired_session();
        let out = send(&mut s, json!({"t": "magnifier", "action": "toggle"}));
        assert!(matches!(
            out.replies[0],
            ServerMessage::MagnifierState { active: true, .. }
        ));
        let _ = MagnifierAction::Off;
    }

    #[test]
    fn garbage_after_authentication_does_not_close_the_session() {
        let (_f, mut s) = paired_session();
        let out = s.on_text("{]");
        assert!(!out.close);
        s.on_binary(&[0xff, 1, 2]);
    }

    #[test]
    fn recenter_is_forwarded() {
        let (f, mut s) = paired_session();
        send(&mut s, json!({"t": "recenter"}));
        assert_eq!(f.input.take(), vec![Event::Recenter]);
    }
}
