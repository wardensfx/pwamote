//! Moteur de pointage : transforme les capteurs du téléphone en déplacements
//! du curseur, à la manière d'une Wiimote.
//!
//! Étapes, pour chaque échantillon :
//! 1. la gravité (accéléromètre filtré passe-bas) donne la verticale réelle ;
//! 2. la vitesse angulaire est projetée sur cette verticale (lacet) et sur
//!    l'axe horizontal droite du téléphone (tangage). Le curseur ne dépend
//!    donc pas de la façon dont on tient le téléphone (roulis compensé) ;
//! 3. un filtre One Euro lisse les tremblements sans ajouter de latence
//!    sur les gestes rapides ;
//! 4. zone morte douce, puis courbe d'accélération ;
//! 5. accumulation sub-pixel pour ne perdre aucun mouvement lent.

use crate::protocol::MotionSample;
use crate::settings::{PointerMode, PointerSettings};

type V3 = [f32; 3];

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: V3) -> f32 {
    dot(a, a).sqrt()
}

fn scale(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn normalize(a: V3) -> Option<V3> {
    let n = norm(a);
    (n > 1e-6).then(|| scale(a, 1.0 / n))
}

/// Constante de temps du filtre de gravité, en secondes.
const GRAVITY_TAU: f32 = 0.12;
/// Au-delà, un trou dans le flux de capteurs n'est pas intégré.
const MAX_DT: f32 = 0.1;
/// Vitesse (°/s) de référence pour la courbe d'accélération.
const ACCEL_REF_DPS: f32 = 90.0;
/// Multiplicateur d'accélération maximal (avant pondération par le réglage).
const ACCEL_MAX: f32 = 4.0;
/// Unités de molette Windows par pixel CSS de défilement.
const WHEEL_PER_PX: f32 = 2.0;

/// Filtre passe-bas à une étape.
#[derive(Debug, Clone, Default)]
struct LowPass {
    y: Option<f32>,
}

impl LowPass {
    fn filter(&mut self, x: f32, alpha: f32) -> f32 {
        let y = match self.y {
            Some(prev) => prev + alpha * (x - prev),
            None => x,
        };
        self.y = Some(y);
        y
    }
}

fn smoothing_alpha(cutoff_hz: f32, dt: f32) -> f32 {
    let tau = 1.0 / (2.0 * std::f32::consts::PI * cutoff_hz);
    1.0 / (1.0 + tau / dt)
}

/// Filtre One Euro (Casiez, Roussel, Vogel, CHI 2012) : fréquence de coupure
/// basse au repos, qui monte avec la vitesse du signal.
#[derive(Debug, Clone, Default)]
pub struct OneEuro {
    x: LowPass,
    dx: LowPass,
    prev: Option<f32>,
}

impl OneEuro {
    pub fn filter(&mut self, x: f32, dt: f32, min_cutoff: f32, beta: f32) -> f32 {
        let raw_d = self.prev.map_or(0.0, |p| (x - p) / dt);
        self.prev = Some(x);
        let d = self.dx.filter(raw_d, smoothing_alpha(1.0, dt));
        let cutoff = min_cutoff + beta * d.abs();
        self.x.filter(x, smoothing_alpha(cutoff, dt))
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Déplacement entier, prêt à injecter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PixelDelta {
    pub dx: i32,
    pub dy: i32,
}

impl PixelDelta {
    pub fn is_zero(&self) -> bool {
        self.dx == 0 && self.dy == 0
    }
}

/// Défilement en unités de molette Windows (120 = un cran).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WheelDelta {
    pub horizontal: i32,
    pub vertical: i32,
}

#[derive(Debug, Clone, Copy, Default)]
struct Accumulator {
    x: f32,
    y: f32,
}

impl Accumulator {
    fn push(&mut self, dx: f32, dy: f32) -> (i32, i32) {
        self.x += dx;
        self.y += dy;
        let (ix, iy) = (self.x.trunc(), self.y.trunc());
        self.x -= ix;
        self.y -= iy;
        (ix as i32, iy as i32)
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug, Clone, Default)]
pub struct PointerEngine {
    up: Option<V3>,
    yaw: OneEuro,
    pitch: OneEuro,
    pixels: Accumulator,
    wheel: Accumulator,
    clutch: bool,
    was_active: bool,
}

impl PointerEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pouce posé ou levé sur la surface du téléphone.
    pub fn set_clutch(&mut self, engaged: bool) {
        self.clutch = engaged;
    }

    pub fn gyro_active(&self, cfg: &PointerSettings) -> bool {
        match cfg.mode {
            PointerMode::AirAlways => true,
            PointerMode::AirHold => self.clutch,
            PointerMode::Trackpad => false,
        }
    }

    /// Verticale « vers le haut » estimée, dans le repère du téléphone.
    pub fn up(&self) -> Option<[f32; 3]> {
        self.up
    }

    fn track_gravity(&mut self, accel: V3, dt: f32) {
        // En chute libre ou sur une trame vide, on garde l'estimation précédente.
        if norm(accel) < 1.0 {
            return;
        }
        let Some(measured) = normalize(accel) else {
            return;
        };
        let next = match self.up {
            None => measured,
            Some(prev) => {
                let a = dt / (GRAVITY_TAU + dt);
                normalize(sub(prev, scale(sub(prev, measured), a))).unwrap_or(measured)
            }
        };
        self.up = Some(next);
    }

    /// Axe horizontal « droite » du téléphone : l'axe x projeté sur le plan
    /// horizontal, ou, si le téléphone est tenu sur la tranche, y × verticale.
    fn right_axis(up: V3) -> V3 {
        let x = [1.0, 0.0, 0.0];
        let projected = sub(x, scale(up, dot(x, up)));
        if norm(projected) > 0.35 {
            return normalize(projected).unwrap_or(x);
        }
        normalize(cross([0.0, 1.0, 0.0], up)).unwrap_or(x)
    }

    pub fn on_motion(&mut self, s: &MotionSample, cfg: &PointerSettings) -> PixelDelta {
        if !(s.dt.is_finite() && s.dt > 0.0) {
            return PixelDelta::default();
        }
        let dt = s.dt.min(MAX_DT);
        self.track_gravity(s.accel, dt);

        if !self.gyro_active(cfg) {
            if self.was_active {
                self.yaw.reset();
                self.pitch.reset();
                self.pixels.clear();
                self.was_active = false;
            }
            return PixelDelta::default();
        }
        self.was_active = true;

        let up = self.up.unwrap_or([0.0, 0.0, 1.0]);
        let omega = s.gyro_dps;
        let yaw_rate = dot(omega, up);
        let pitch_rate = dot(omega, Self::right_axis(up));

        let min_cutoff = 8.0 + (0.6 - 8.0) * cfg.smoothing;
        let beta = 0.006;
        let yaw = self.yaw.filter(yaw_rate, dt, min_cutoff, beta);
        let pitch = self.pitch.filter(pitch_rate, dt, min_cutoff, beta);

        let speed = yaw.hypot(pitch);
        let deadzone = if cfg.deadzone_dps > 0.0 {
            (speed / cfg.deadzone_dps).min(1.0).powi(2)
        } else {
            1.0
        };
        let accel = 1.0 + cfg.acceleration * (speed / ACCEL_REF_DPS).min(ACCEL_MAX);
        let gain = cfg.sensitivity * dt * deadzone * accel;

        // Tourner vers la gauche (lacet positif) envoie le curseur à gauche ;
        // lever le bout du téléphone (tangage positif) l'envoie vers le haut.
        let dx = -yaw * gain * if cfg.invert_x { -1.0 } else { 1.0 };
        let dy = -pitch * gain * if cfg.invert_y { -1.0 } else { 1.0 };
        let (dx, dy) = self.pixels.push(dx, dy);
        PixelDelta { dx, dy }
    }

    pub fn on_trackpad(&mut self, dx: f32, dy: f32, cfg: &PointerSettings) -> PixelDelta {
        let distance = dx.hypot(dy);
        // Distance par événement : un geste rapide (≈ 16 px à 60 Hz) multiplie jusqu'à ×(1 + 4·accélération).
        let accel = 1.0 + cfg.acceleration * (distance / 4.0).min(4.0);
        let k = cfg.trackpad_speed * accel;
        let (dx, dy) = self.pixels.push(dx * k, dy * k);
        PixelDelta { dx, dy }
    }

    pub fn on_scroll(&mut self, dx: f32, dy: f32, cfg: &PointerSettings) -> WheelDelta {
        let k = WHEEL_PER_PX * cfg.scroll_speed * if cfg.natural_scroll { 1.0 } else { -1.0 };
        let (h, v) = self.wheel.push(-dx * k, dy * k);
        WheelDelta {
            horizontal: h,
            vertical: v,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: f32 = 9.81;
    const DT: f32 = 1.0 / 60.0;

    fn cfg() -> PointerSettings {
        PointerSettings {
            mode: PointerMode::AirAlways,
            ..PointerSettings::default()
        }
    }

    /// Envoie `n` échantillons identiques et somme les déplacements.
    fn run(
        engine: &mut PointerEngine,
        gyro: V3,
        accel: V3,
        n: usize,
        cfg: &PointerSettings,
    ) -> (i32, i32) {
        let mut total = (0, 0);
        for _ in 0..n {
            let d = engine.on_motion(
                &MotionSample {
                    gyro_dps: gyro,
                    accel,
                    dt: DT,
                },
                cfg,
            );
            total.0 += d.dx;
            total.1 += d.dy;
        }
        total
    }

    const FLAT: V3 = [0.0, 0.0, G];

    #[test]
    fn turning_left_moves_the_cursor_left() {
        let mut e = PointerEngine::new();
        // Téléphone à plat, rotation positive autour de la verticale (z) = vers la gauche.
        let (dx, dy) = run(&mut e, [0.0, 0.0, 40.0], FLAT, 30, &cfg());
        assert!(dx < -100, "dx = {dx}");
        assert_eq!(dy, 0);
    }

    #[test]
    fn raising_the_tip_moves_the_cursor_up() {
        let mut e = PointerEngine::new();
        let (dx, dy) = run(&mut e, [40.0, 0.0, 0.0], FLAT, 30, &cfg());
        assert_eq!(dx, 0);
        assert!(dy < -100, "dy = {dy}");
    }

    #[test]
    fn invert_y_flips_vertical_motion_only() {
        let mut e = PointerEngine::new();
        let c = PointerSettings {
            invert_y: true,
            ..cfg()
        };
        let (_, dy) = run(&mut e, [40.0, 0.0, 0.0], FLAT, 30, &c);
        assert!(dy > 100);
    }

    #[test]
    fn roll_is_compensated_for_yaw() {
        // Téléphone tourné de 90° sur le côté (écran vers la gauche) : la verticale
        // est maintenant l'axe x du téléphone. Tourner vers la gauche autour de la
        // verticale du monde doit toujours envoyer le curseur à gauche.
        let rolled_up = [G, 0.0, 0.0];
        let mut e = PointerEngine::new();
        let (dx, dy) = run(&mut e, [40.0, 0.0, 0.0], rolled_up, 30, &cfg());
        assert!(dx < -100, "dx = {dx}");
        assert_eq!(dy, 0);
    }

    #[test]
    fn roll_by_45_degrees_keeps_axes_separate() {
        let c = std::f32::consts::FRAC_1_SQRT_2;
        let up = [c * G, 0.0, c * G];
        let yaw_axis = [c * 40.0, 0.0, c * 40.0];
        let mut e = PointerEngine::new();
        let (dx, dy) = run(&mut e, yaw_axis, up, 30, &cfg());
        assert!(dx < -100);
        assert!(dy.abs() <= 1, "dy = {dy}");
    }

    #[test]
    fn upright_phone_pitches_with_its_x_axis() {
        // Téléphone tenu debout face à soi : la verticale est l'axe y.
        let mut e = PointerEngine::new();
        let (dx, dy) = run(&mut e, [40.0, 0.0, 0.0], [0.0, G, 0.0], 30, &cfg());
        assert_eq!(dx, 0);
        assert!(dy < -100);
    }

    #[test]
    fn tremor_below_the_deadzone_is_absorbed() {
        let mut e = PointerEngine::new();
        let (dx, dy) = run(&mut e, [0.3, 0.0, 0.3], FLAT, 120, &cfg());
        assert!(dx.abs() <= 1 && dy.abs() <= 1, "({dx}, {dy})");
    }

    #[test]
    fn slow_motion_is_not_lost_by_rounding() {
        // 3°/s pendant 2 s = 6° ; à 38 px/° on attend environ 228 px moins la zone morte.
        let c = PointerSettings {
            acceleration: 0.0,
            smoothing: 0.0,
            deadzone_dps: 0.0,
            ..cfg()
        };
        let mut e = PointerEngine::new();
        let (dx, _) = run(&mut e, [0.0, 0.0, 3.0], FLAT, 120, &c);
        assert!((-232..=-220).contains(&dx), "dx = {dx}");
    }

    #[test]
    fn acceleration_makes_fast_gestures_travel_further() {
        let lin = PointerSettings {
            acceleration: 0.0,
            ..cfg()
        };
        let acc = PointerSettings {
            acceleration: 1.0,
            ..cfg()
        };
        let (fast_lin, _) = run(&mut PointerEngine::new(), [0.0, 0.0, 200.0], FLAT, 10, &lin);
        let (fast_acc, _) = run(&mut PointerEngine::new(), [0.0, 0.0, 200.0], FLAT, 10, &acc);
        assert!(
            fast_acc.abs() > 2 * fast_lin.abs(),
            "{fast_acc} vs {fast_lin}"
        );
    }

    #[test]
    fn hold_mode_only_moves_while_clutched() {
        let c = PointerSettings {
            mode: PointerMode::AirHold,
            ..cfg()
        };
        let mut e = PointerEngine::new();
        assert_eq!(run(&mut e, [0.0, 0.0, 40.0], FLAT, 20, &c), (0, 0));
        e.set_clutch(true);
        assert!(run(&mut e, [0.0, 0.0, 40.0], FLAT, 20, &c).0 < 0);
        e.set_clutch(false);
        assert_eq!(run(&mut e, [0.0, 0.0, 40.0], FLAT, 20, &c), (0, 0));
    }

    #[test]
    fn trackpad_mode_ignores_the_gyroscope() {
        let c = PointerSettings {
            mode: PointerMode::Trackpad,
            ..cfg()
        };
        let mut e = PointerEngine::new();
        e.set_clutch(true);
        assert_eq!(run(&mut e, [0.0, 0.0, 90.0], FLAT, 20, &c), (0, 0));
    }

    #[test]
    fn invalid_dt_is_ignored_and_long_gaps_are_capped() {
        let mut e = PointerEngine::new();
        let c = PointerSettings {
            smoothing: 0.0,
            acceleration: 0.0,
            deadzone_dps: 0.0,
            ..cfg()
        };
        for dt in [0.0, -1.0, f32::NAN] {
            let d = e.on_motion(
                &MotionSample {
                    gyro_dps: [0.0, 0.0, 100.0],
                    accel: FLAT,
                    dt,
                },
                &c,
            );
            assert!(d.is_zero());
        }
        let d = e.on_motion(
            &MotionSample {
                gyro_dps: [0.0, 0.0, 10.0],
                accel: FLAT,
                dt: 5.0,
            },
            &c,
        );
        // Plafonné à 0,1 s : 10°/s × 0,1 s × 38 px/° = 38 px.
        assert_eq!(d.dx, -38);
    }

    #[test]
    fn gravity_estimate_survives_hand_shake() {
        let mut e = PointerEngine::new();
        let c = cfg();
        run(&mut e, [0.0; 3], FLAT, 30, &c);
        // Une secousse latérale brève ne fait presque pas bouger la verticale.
        run(&mut e, [0.0; 3], [6.0, 0.0, G], 2, &c);
        let up = e.up().unwrap();
        assert!(up[2] > 0.95, "{up:?}");
    }

    #[test]
    fn trackpad_moves_with_speed_and_keeps_sub_pixels() {
        let c = PointerSettings {
            trackpad_speed: 1.0,
            acceleration: 0.0,
            ..cfg()
        };
        let mut e = PointerEngine::new();
        let mut sum = 0;
        for _ in 0..10 {
            sum += e.on_trackpad(0.25, 0.0, &c).dx;
        }
        assert_eq!(sum, 2);
    }

    #[test]
    fn invert_x_flips_horizontal_motion_only() {
        let mut e = PointerEngine::new();
        let c = PointerSettings {
            invert_x: true,
            ..cfg()
        };
        let (dx, dy) = run(&mut e, [0.0, 0.0, 40.0], FLAT, 30, &c);
        assert!(dx > 100, "dx = {dx}");
        assert_eq!(dy, 0);
    }

    #[test]
    fn a_fast_swipe_at_default_settings_crosses_most_of_a_full_hd_screen() {
        // Pad d'environ 360 px de large, balayé en 20 événements : de bord à bord.
        let c = PointerSettings::default();
        let mut e = PointerEngine::new();
        let total: i32 = (0..20).map(|_| e.on_trackpad(16.0, 0.0, &c).dx).sum();
        assert!(total > 1500, "total = {total}");
    }

    #[test]
    fn slow_trackpad_movement_stays_precise() {
        let c = PointerSettings {
            acceleration: 0.45,
            trackpad_speed: 3.0,
            ..PointerSettings::default()
        };
        let mut e = PointerEngine::new();
        let total: i32 = (0..20).map(|_| e.on_trackpad(0.5, 0.0, &c).dx).sum();
        // 0,5 px × 3 de vitesse × ~1,06 d’accélération × 20 événements ≈ 31 : quasi linéaire.
        assert!((28..=34).contains(&total), "total = {total}");
    }

    #[test]
    fn natural_scroll_follows_the_finger() {
        let c = PointerSettings {
            natural_scroll: true,
            scroll_speed: 1.0,
            ..cfg()
        };
        let mut e = PointerEngine::new();
        // Doigt qui descend : le contenu descend, donc on remonte (molette positive).
        assert!(e.on_scroll(0.0, 30.0, &c).vertical > 0);
        let c = PointerSettings {
            natural_scroll: false,
            ..c
        };
        assert!(e.on_scroll(0.0, 30.0, &c).vertical < 0);
    }

    #[test]
    fn one_euro_converges_to_a_constant_signal() {
        let mut f = OneEuro::default();
        let mut y = 0.0;
        for i in 0..200 {
            y = f.filter(if i == 0 { 0.0 } else { 10.0 }, DT, 1.0, 0.0);
        }
        assert!((y - 10.0).abs() < 0.01);
    }
}
