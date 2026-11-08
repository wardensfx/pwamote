//! Injection de saisie Windows via `SendInput`.

use pwamote_core::pointer::WheelDelta;
use pwamote_core::protocol::{Key, Modifier, MouseButton};
use windows::core::HSTRING;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SetCursorPos, SM_CXSCREEN, SM_CYSCREEN, SW_SHOWNORMAL,
};

use crate::input::InputSink;

pub struct WindowsInput;

/// Code de touche virtuelle pour une touche nommée.
pub fn virtual_key(key: Key) -> Option<VIRTUAL_KEY> {
    Some(match key {
        Key::Escape => VK_ESCAPE,
        Key::Tab => VK_TAB,
        Key::Enter => VK_RETURN,
        Key::Backspace => VK_BACK,
        Key::Delete => VK_DELETE,
        Key::Space => VK_SPACE,
        Key::ArrowUp => VK_UP,
        Key::ArrowDown => VK_DOWN,
        Key::ArrowLeft => VK_LEFT,
        Key::ArrowRight => VK_RIGHT,
        Key::Home => VK_HOME,
        Key::End => VK_END,
        Key::PageUp => VK_PRIOR,
        Key::PageDown => VK_NEXT,
        Key::Insert => VK_INSERT,
        Key::F1 => VK_F1,
        Key::F2 => VK_F2,
        Key::F3 => VK_F3,
        Key::F4 => VK_F4,
        Key::F5 => VK_F5,
        Key::F6 => VK_F6,
        Key::F7 => VK_F7,
        Key::F8 => VK_F8,
        Key::F9 => VK_F9,
        Key::F10 => VK_F10,
        Key::F11 => VK_F11,
        Key::F12 => VK_F12,
        Key::PrintScreen => VK_SNAPSHOT,
        Key::VolumeUp => VK_VOLUME_UP,
        Key::VolumeDown => VK_VOLUME_DOWN,
        Key::VolumeMute => VK_VOLUME_MUTE,
        Key::MediaPlayPause => VK_MEDIA_PLAY_PAUSE,
        Key::MediaNext => VK_MEDIA_NEXT_TRACK,
        Key::MediaPrevious => VK_MEDIA_PREV_TRACK,
        // Lettres et chiffres : le code virtuel est le code ASCII majuscule.
        Key::Char(c) if c.is_ascii_alphanumeric() => VIRTUAL_KEY(c.to_ascii_uppercase() as u16),
        Key::Char(_) => return None,
    })
}

fn modifier_vk(m: Modifier) -> VIRTUAL_KEY {
    match m {
        Modifier::Ctrl => VK_CONTROL,
        Modifier::Shift => VK_SHIFT,
        Modifier::Alt => VK_MENU,
        Modifier::Meta => VK_LWIN,
    }
}

/// Ces touches sont des touches « étendues » : sans le drapeau, Windows
/// les confond avec celles du pavé numérique.
fn is_extended(vk: VIRTUAL_KEY) -> bool {
    matches!(
        vk,
        VK_UP
            | VK_DOWN
            | VK_LEFT
            | VK_RIGHT
            | VK_HOME
            | VK_END
            | VK_PRIOR
            | VK_NEXT
            | VK_INSERT
            | VK_DELETE
            | VK_LWIN
    )
}

fn key_input(vk: VIRTUAL_KEY, down: bool) -> INPUT {
    // Le code de balayage en plus du code virtuel : les navigateurs en déduisent `KeyboardEvent.code`,
    // que certaines extensions (vitesse des vidéos…) lisent pour reconnaître une touche.
    let scan = unsafe { MapVirtualKeyW(u32::from(vk.0), MAPVK_VK_TO_VSC) } as u16;
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }
    if is_extended(vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn unicode_input(unit: u16, down: bool) -> INPUT {
    let mut flags = KEYEVENTF_UNICODE;
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: unit,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn mouse_input(flags: MOUSE_EVENT_FLAGS, dx: i32, dy: i32, data: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send(inputs: &[INPUT]) {
    if inputs.is_empty() {
        return;
    }
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if (sent as usize) != inputs.len() {
        // Typiquement : fenêtre élevée (administrateur) ou écran de verrouillage.
        tracing::warn!(
            sent,
            expected = inputs.len(),
            "SendInput a refusé une partie de la saisie"
        );
    }
}

impl InputSink for WindowsInput {
    fn move_by(&self, dx: i32, dy: i32) {
        send(&[mouse_input(MOUSEEVENTF_MOVE, dx, dy, 0)]);
    }

    fn button(&self, button: MouseButton, down: bool) {
        let flags = match (button, down) {
            (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
            (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
            (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
            (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
            (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
            (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
        };
        send(&[mouse_input(flags, 0, 0, 0)]);
    }

    fn wheel(&self, delta: WheelDelta) {
        let mut inputs = Vec::new();
        if delta.vertical != 0 {
            inputs.push(mouse_input(MOUSEEVENTF_WHEEL, 0, 0, delta.vertical));
        }
        if delta.horizontal != 0 {
            inputs.push(mouse_input(MOUSEEVENTF_HWHEEL, 0, 0, delta.horizontal));
        }
        send(&inputs);
    }

    fn key(&self, key: Key, down: bool) {
        match virtual_key(key) {
            Some(vk) => send(&[key_input(vk, down)]),
            None => {
                // Caractère sans touche dédiée : on le tape en Unicode, à l'appui seulement.
                if let (Key::Char(c), true) = (key, down) {
                    self.text(&c.to_string());
                }
            }
        }
    }

    fn modifier(&self, modifier: Modifier, down: bool) {
        send(&[key_input(modifier_vk(modifier), down)]);
    }

    fn text(&self, text: &str) {
        let mut inputs = Vec::with_capacity(text.len() * 2);
        for unit in text.encode_utf16() {
            inputs.push(unicode_input(unit, true));
            inputs.push(unicode_input(unit, false));
        }
        // Par paquets, pour ne pas dépasser la file d'entrée sur un long texte collé.
        for chunk in inputs.chunks(512) {
            send(chunk);
        }
    }

    fn recenter(&self) {
        unsafe {
            let (w, h) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
            let _ = SetCursorPos(w / 2, h / 2);
        }
    }
}

/// Ouvre une adresse, un fichier ou un dossier avec l'application associée.
pub fn shell_open(target: &str) -> Result<(), String> {
    let result = unsafe {
        ShellExecuteW(
            None,
            &HSTRING::from("open"),
            &HSTRING::from(target),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    // Par convention historique, une valeur supérieure à 32 signifie le succès.
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err(format!("Windows n'a pas pu ouvrir « {target} »"))
    }
}
