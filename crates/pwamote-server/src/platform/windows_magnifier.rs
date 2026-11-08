//! Loupe Windows : API Magnification en plein écran, pilotée à 60 Hz par le
//! `MagnifierController` du cœur (animation, suivi du curseur et du caret).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pwamote_core::magnifier::{MagnifierController, Observation, Point, Rect};
use pwamote_core::protocol::MagnifierAction;
use pwamote_core::settings::MagnifierSettings;
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::Magnification::{MagInitialize, MagSetFullscreenTransform, MagUninitialize};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetGUIThreadInfo, GetSystemMetrics, GUITHREADINFO, SM_CXSCREEN, SM_CYSCREEN,
};

use crate::input::{apply_action, MagnifierSink, MagnifierStatus};

struct Shared {
    controller: MagnifierController,
    settings: MagnifierSettings,
}

pub struct WindowsMagnifier {
    shared: Arc<Mutex<Shared>>,
}

fn primary_screen() -> Rect {
    unsafe {
        Rect { x: 0.0, y: 0.0, w: GetSystemMetrics(SM_CXSCREEN) as f32, h: GetSystemMetrics(SM_CYSCREEN) as f32 }
    }
}

fn cursor() -> Point {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    Point { x: p.x as f32, y: p.y as f32 }
}

/// Position du caret de saisie de la fenêtre active, si elle en expose un.
/// Fonctionne pour les applications Win32 classiques ; les applications qui
/// dessinent leur propre caret (navigateurs, Electron) retombent sur le curseur.
fn caret() -> Option<Rect> {
    unsafe {
        let mut info = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
        GetGUIThreadInfo(0, &mut info).ok()?;
        if info.hwndCaret.0.is_null() {
            return None;
        }
        let RECT { left, top, right, bottom } = info.rcCaret;
        let mut origin = POINT { x: left, y: top };
        if !ClientToScreen(info.hwndCaret, &mut origin).as_bool() {
            return None;
        }
        Some(Rect {
            x: origin.x as f32,
            y: origin.y as f32,
            w: (right - left).max(1) as f32,
            h: (bottom - top).max(1) as f32,
        })
    }
}

impl WindowsMagnifier {
    pub fn start(settings: &MagnifierSettings) -> Self {
        let shared = Arc::new(Mutex::new(Shared {
            controller: MagnifierController::new(settings),
            settings: settings.clone(),
        }));
        let worker = shared.clone();
        std::thread::Builder::new()
            .name("magnifier".into())
            .spawn(move || run(worker))
            .expect("thread de la loupe");
        Self { shared }
    }
}

fn run(shared: Arc<Mutex<Shared>>) {
    if !unsafe { MagInitialize() }.as_bool() {
        tracing::error!("MagInitialize a échoué : la loupe est indisponible");
        return;
    }
    let started = Instant::now();
    let mut last = Instant::now();
    let mut showing = false;
    loop {
        std::thread::sleep(Duration::from_millis(16));
        let now = Instant::now();
        let dt_ms = now.duration_since(last).as_secs_f32() * 1000.0;
        last = now;

        let transform = {
            let mut s = shared.lock().expect("magnifier lock");
            let cfg = s.settings.clone();
            let obs = Observation {
                screen: primary_screen(),
                cursor: cursor(),
                caret: caret(),
                now_ms: started.elapsed().as_millis() as u64,
            };
            s.controller.step(&obs, dt_ms, &cfg)
        };
        match transform {
            Some(t) => {
                showing = true;
                unsafe {
                    let _ = MagSetFullscreenTransform(t.zoom, t.x.round() as i32, t.y.round() as i32);
                }
            }
            None if showing => {
                showing = false;
                unsafe {
                    let _ = MagSetFullscreenTransform(1.0, 0, 0);
                }
            }
            None => {}
        }
    }
    #[allow(unreachable_code)]
    unsafe {
        let _ = MagUninitialize();
    }
}

impl MagnifierSink for WindowsMagnifier {
    fn command(&self, action: MagnifierAction) -> MagnifierStatus {
        let mut s = self.shared.lock().expect("magnifier lock");
        apply_action(&mut s.controller, action);
        MagnifierStatus { active: s.controller.is_active(), zoom: s.controller.target_zoom() }
    }

    fn configure(&self, settings: &MagnifierSettings) {
        let mut s = self.shared.lock().expect("magnifier lock");
        s.controller.set_zoom(settings.zoom);
        s.settings = settings.clone();
    }
}
