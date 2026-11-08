//! Loupe intelligente : calcule, à chaque image, le niveau de zoom et la zone
//! d'écran à grossir. Le serveur applique le résultat avec l'API Windows
//! Magnification (`MagSetFullscreenTransform`).
//!
//! * Le zoom s'anime vers sa cible (entrée et sortie douces).
//! * La zone suit le point d'intérêt avec une marge : elle ne se déplace que
//!   lorsque ce point s'approche d'un bord, ce qui évite le mal de mer.
//! * En mode `smart`, le point d'intérêt est le caret de saisie pendant la
//!   frappe, puis redevient le curseur.

use crate::settings::{MagnifierFollow, MagnifierSettings};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn center(&self) -> Point {
        Point {
            x: self.x + self.w / 2.0,
            y: self.y + self.h / 2.0,
        }
    }
}

/// Transformation plein écran : grossissement et coin haut-gauche de la zone source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub zoom: f32,
    pub x: f32,
    pub y: f32,
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        zoom: 1.0,
        x: 0.0,
        y: 0.0,
    };
}

/// Ce que le serveur observe à chaque image.
#[derive(Debug, Clone, Copy)]
pub struct Observation {
    /// Écran (moniteur) sur lequel se trouve le point d'intérêt.
    pub screen: Rect,
    pub cursor: Point,
    /// Position du caret de saisie, si une zone de texte a le focus.
    pub caret: Option<Rect>,
    /// Horodatage en millisecondes.
    pub now_ms: u64,
}

/// Part de la zone visible, de chaque côté, dans laquelle le point d'intérêt
/// peut bouger sans faire défiler la loupe.
const FOLLOW_MARGIN: f32 = 0.22;
pub const MIN_ZOOM: f32 = 1.25;
pub const MAX_ZOOM: f32 = 8.0;

#[derive(Debug, Clone)]
pub struct MagnifierController {
    active: bool,
    zoom_target: f32,
    zoom: f32,
    /// Coin haut-gauche de la zone source.
    origin: Option<Point>,
    /// Taille de la zone source à l'image précédente.
    view: Option<Point>,
    last_caret: Option<Rect>,
    last_caret_change_ms: Option<u64>,
}

impl MagnifierController {
    pub fn new(cfg: &MagnifierSettings) -> Self {
        Self {
            active: false,
            zoom_target: cfg.zoom,
            zoom: 1.0,
            origin: None,
            view: None,
            last_caret: None,
            last_caret_change_ms: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn target_zoom(&self) -> f32 {
        self.zoom_target
    }

    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    pub fn toggle(&mut self) {
        self.active = !self.active;
    }

    pub fn zoom_by(&mut self, factor: f32) {
        self.zoom_target = (self.zoom_target * factor).clamp(MIN_ZOOM, MAX_ZOOM);
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom_target = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    }

    /// Le point que la loupe doit garder visible.
    pub fn focus(&mut self, obs: &Observation, cfg: &MagnifierSettings) -> Point {
        if obs.caret != self.last_caret {
            if obs.caret.is_some() {
                self.last_caret_change_ms = Some(obs.now_ms);
            }
            self.last_caret = obs.caret;
        }
        let caret = obs.caret.map(|c| c.center());
        match (cfg.follow, caret) {
            (MagnifierFollow::Cursor, _) | (_, None) => obs.cursor,
            (MagnifierFollow::Caret, Some(c)) => c,
            (MagnifierFollow::Smart, Some(c)) => {
                let typing = self
                    .last_caret_change_ms
                    .is_some_and(|t| obs.now_ms.saturating_sub(t) <= cfg.caret_hold_ms as u64);
                if typing {
                    c
                } else {
                    obs.cursor
                }
            }
        }
    }

    /// Avance d'une image. Renvoie `None` quand la loupe est complètement fermée.
    pub fn step(
        &mut self,
        obs: &Observation,
        dt_ms: f32,
        cfg: &MagnifierSettings,
    ) -> Option<Transform> {
        let focus = self.focus(obs, cfg);
        let goal = if self.active { self.zoom_target } else { 1.0 };
        let duration = (cfg.animation_ms as f32).max(1.0);
        // Approche exponentielle : ~95 % du chemin parcouru en `animation_ms`.
        let k = 1.0 - (-3.0 * dt_ms.max(0.0) / duration).exp();
        self.zoom += (goal - self.zoom) * k;
        if (self.zoom - goal).abs() < 0.002 {
            self.zoom = goal;
        }
        if self.zoom <= 1.0 + f32::EPSILON {
            self.zoom = 1.0;
            self.origin = None;
            self.view = None;
            return if self.active {
                Some(Transform::IDENTITY)
            } else {
                None
            };
        }

        let s = obs.screen;
        let view_w = s.w / self.zoom;
        let view_h = s.h / self.zoom;
        // Quand le zoom change, la position relative du point d'intérêt dans la
        // zone visible est conservée : le zoom se fait « autour » de lui.
        let origin = match (self.origin, self.view) {
            (Some(o), Some(v)) => {
                let ux = ((focus.x - o.x) / v.x).clamp(0.0, 1.0);
                let uy = ((focus.y - o.y) / v.y).clamp(0.0, 1.0);
                Point {
                    x: focus.x - ux * view_w,
                    y: focus.y - uy * view_h,
                }
            }
            _ => Point {
                x: focus.x - view_w / 2.0,
                y: focus.y - view_h / 2.0,
            },
        };

        let follow = |o: f32, f: f32, view: f32| -> f32 {
            let margin = view * FOLLOW_MARGIN;
            if f < o + margin {
                f - margin
            } else if f > o + view - margin {
                f - view + margin
            } else {
                o
            }
        };
        let x = follow(origin.x, focus.x, view_w).clamp(s.x, s.x + s.w - view_w);
        let y = follow(origin.y, focus.y, view_h).clamp(s.y, s.y + s.h - view_h);
        self.origin = Some(Point { x, y });
        self.view = Some(Point {
            x: view_w,
            y: view_h,
        });
        Some(Transform {
            zoom: self.zoom,
            x,
            y,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1920.0,
        h: 1080.0,
    };

    fn cfg() -> MagnifierSettings {
        MagnifierSettings {
            zoom: 2.0,
            animation_ms: 100,
            ..MagnifierSettings::default()
        }
    }

    fn obs(cursor: (f32, f32), caret: Option<Rect>, now_ms: u64) -> Observation {
        Observation {
            screen: SCREEN,
            cursor: Point {
                x: cursor.0,
                y: cursor.1,
            },
            caret,
            now_ms,
        }
    }

    fn settle(
        m: &mut MagnifierController,
        o: &Observation,
        c: &MagnifierSettings,
    ) -> Option<Transform> {
        let mut t = None;
        for _ in 0..200 {
            t = m.step(o, 16.0, c);
        }
        t
    }

    #[test]
    fn inactive_magnifier_produces_nothing() {
        let c = cfg();
        let mut m = MagnifierController::new(&c);
        assert_eq!(m.step(&obs((10.0, 10.0), None, 0), 16.0, &c), None);
    }

    #[test]
    fn zoom_animates_towards_its_target() {
        let c = cfg();
        let mut m = MagnifierController::new(&c);
        m.set_active(true);
        let first = m.step(&obs((960.0, 540.0), None, 0), 16.0, &c).unwrap();
        assert!(first.zoom > 1.0 && first.zoom < 2.0, "{first:?}");
        let end = settle(&mut m, &obs((960.0, 540.0), None, 0), &c).unwrap();
        assert_eq!(end.zoom, 2.0);
    }

    #[test]
    fn view_is_centered_on_the_cursor_and_clamped_to_the_screen() {
        let c = cfg();
        let mut m = MagnifierController::new(&c);
        m.set_active(true);
        let t = settle(&mut m, &obs((960.0, 540.0), None, 0), &c).unwrap();
        assert_eq!((t.x, t.y), (480.0, 270.0));

        let mut m = MagnifierController::new(&c);
        m.set_active(true);
        let t = settle(&mut m, &obs((5.0, 1075.0), None, 0), &c).unwrap();
        assert_eq!((t.x, t.y), (0.0, 540.0));
    }

    #[test]
    fn small_cursor_moves_do_not_pan_the_view() {
        let c = cfg();
        let mut m = MagnifierController::new(&c);
        m.set_active(true);
        let a = settle(&mut m, &obs((960.0, 540.0), None, 0), &c).unwrap();
        let b = m.step(&obs((1000.0, 560.0), None, 0), 16.0, &c).unwrap();
        assert_eq!((a.x, a.y), (b.x, b.y));
    }

    #[test]
    fn view_follows_when_the_cursor_nears_an_edge() {
        let c = cfg();
        let mut m = MagnifierController::new(&c);
        m.set_active(true);
        let a = settle(&mut m, &obs((960.0, 540.0), None, 0), &c).unwrap();
        let b = m.step(&obs((1400.0, 540.0), None, 0), 16.0, &c).unwrap();
        assert!(b.x > a.x);
        // Le curseur reste dans la zone visible, hors marge.
        let view_w = 1920.0 / b.zoom;
        assert!(1400.0 <= b.x + view_w * (1.0 - FOLLOW_MARGIN) + 0.01);
    }

    #[test]
    fn smart_mode_follows_the_caret_while_typing_then_the_cursor() {
        let c = MagnifierSettings {
            follow: MagnifierFollow::Smart,
            caret_hold_ms: 1000,
            ..cfg()
        };
        let mut m = MagnifierController::new(&c);
        let caret = Rect {
            x: 100.0,
            y: 900.0,
            w: 2.0,
            h: 20.0,
        };
        let p = m.focus(&obs((1500.0, 200.0), Some(caret), 0), &c);
        assert_eq!(p, caret.center());
        // Le caret ne bouge plus depuis 2 s : on revient au curseur.
        let p = m.focus(&obs((1500.0, 200.0), Some(caret), 2000), &c);
        assert_eq!(
            p,
            Point {
                x: 1500.0,
                y: 200.0
            }
        );
        // Nouvelle frappe : le caret bouge, on le suit de nouveau.
        let moved = Rect { x: 110.0, ..caret };
        let p = m.focus(&obs((1500.0, 200.0), Some(moved), 2100), &c);
        assert_eq!(p, moved.center());
    }

    #[test]
    fn cursor_mode_ignores_the_caret() {
        let c = MagnifierSettings {
            follow: MagnifierFollow::Cursor,
            ..cfg()
        };
        let mut m = MagnifierController::new(&c);
        let caret = Rect {
            x: 100.0,
            y: 900.0,
            w: 2.0,
            h: 20.0,
        };
        assert_eq!(
            m.focus(&obs((5.0, 6.0), Some(caret), 0), &c),
            Point { x: 5.0, y: 6.0 }
        );
    }

    #[test]
    fn closing_animates_back_to_nothing() {
        let c = cfg();
        let mut m = MagnifierController::new(&c);
        m.set_active(true);
        settle(&mut m, &obs((960.0, 540.0), None, 0), &c);
        m.set_active(false);
        let mid = m.step(&obs((960.0, 540.0), None, 0), 16.0, &c).unwrap();
        assert!(mid.zoom < 2.0);
        assert_eq!(settle(&mut m, &obs((960.0, 540.0), None, 0), &c), None);
    }

    #[test]
    fn zoom_is_bounded() {
        let c = cfg();
        let mut m = MagnifierController::new(&c);
        for _ in 0..50 {
            m.zoom_by(1.5);
        }
        assert_eq!(m.target_zoom(), MAX_ZOOM);
        for _ in 0..50 {
            m.zoom_by(0.5);
        }
        assert_eq!(m.target_zoom(), MIN_ZOOM);
    }

    #[test]
    fn works_on_a_secondary_monitor() {
        let c = cfg();
        let mut m = MagnifierController::new(&c);
        m.set_active(true);
        let right = Rect {
            x: 1920.0,
            y: 0.0,
            w: 2560.0,
            h: 1440.0,
        };
        let o = Observation {
            screen: right,
            cursor: Point { x: 1925.0, y: 10.0 },
            caret: None,
            now_ms: 0,
        };
        let t = settle(&mut m, &o, &c).unwrap();
        assert_eq!((t.x, t.y), (1920.0, 0.0));
    }
}
