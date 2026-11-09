//! État partagé du serveur : réglages, appareils appairés, ports système.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use pwamote_core::actions::{ActionError, Deck, DeckView, InputValue};
use pwamote_core::pairing::{Device, PairingError, PairingRegistry};
use pwamote_core::settings::{Conflict, Settings, SettingsStore, VersionedSettings};
use rand::Rng as _;
use tokio::sync::broadcast;

use crate::actions::{ActionJob, Launcher, MAX_CONCURRENT_ACTIONS};
use crate::clipboard::{ClipboardError, ClipboardSink};
use crate::config::AppConfig;
use crate::input::{InputSink, MagnifierSink};

/// Persistance des réglages et des appareils.
pub trait Persist: Send + Sync {
    fn save_settings(&self, settings: &VersionedSettings);
    fn save_devices(&self, registry: &PairingRegistry);
    fn save_config(&self, config: &AppConfig);
    fn save_deck(&self, deck: &Deck);
}

/// Fichiers JSON dans le dossier de données de l'utilisateur
/// (`%APPDATA%\PwaMote` sous Windows).
pub struct JsonFiles {
    dir: PathBuf,
}

const SETTINGS_FILE: &str = "settings.json";
const DEVICES_FILE: &str = "devices.json";
const CONFIG_FILE: &str = "config.json";
const DECK_FILE: &str = "actions.json";

impl JsonFiles {
    pub fn new(dir: impl Into<PathBuf>) -> std::io::Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn read<T: serde::de::DeserializeOwned>(&self, name: &str) -> Option<T> {
        let raw = std::fs::read_to_string(self.dir.join(name)).ok()?;
        match serde_json::from_str(&raw) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!(file = name, error = %e, "fichier illisible, valeurs par défaut utilisées");
                None
            }
        }
    }

    /// Écriture atomique : fichier temporaire puis renommage.
    fn write<T: serde::Serialize>(&self, name: &str, value: &T) {
        let result = (|| -> std::io::Result<()> {
            let tmp = self.dir.join(format!("{name}.tmp"));
            std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
            std::fs::rename(tmp, self.dir.join(name))
        })();
        if let Err(e) = result {
            tracing::error!(file = name, error = %e, "échec d'écriture");
        }
    }

    pub fn load_settings(&self) -> VersionedSettings {
        self.read(SETTINGS_FILE).unwrap_or(VersionedSettings {
            rev: 0,
            settings: Settings::default(),
        })
    }

    pub fn load_config(&self) -> AppConfig {
        self.read::<AppConfig>(CONFIG_FILE)
            .unwrap_or_default()
            .sanitized()
    }

    pub fn load_devices(&self) -> PairingRegistry {
        self.read(DEVICES_FILE).unwrap_or_default()
    }

    /// Le panneau d'actions ; au tout premier lancement, un exemple à adapter.
    pub fn load_deck(&self) -> Deck {
        if !self.dir.join(DECK_FILE).exists() {
            return Deck::example();
        }
        self.read(DECK_FILE).unwrap_or_default()
    }
}

impl Persist for JsonFiles {
    fn save_settings(&self, settings: &VersionedSettings) {
        self.write(SETTINGS_FILE, settings);
    }
    fn save_devices(&self, registry: &PairingRegistry) {
        self.write(DEVICES_FILE, registry);
    }
    fn save_config(&self, config: &AppConfig) {
        self.write(CONFIG_FILE, config);
    }
    fn save_deck(&self, deck: &Deck) {
        self.write(DECK_FILE, deck);
    }
}

/// Persistance en mémoire, pour les tests.
#[derive(Default)]
pub struct MemoryPersist {
    pub settings: Mutex<Option<VersionedSettings>>,
    pub devices: Mutex<Option<PairingRegistry>>,
    pub config: Mutex<Option<AppConfig>>,
    pub deck: Mutex<Option<Deck>>,
}

impl Persist for MemoryPersist {
    fn save_settings(&self, settings: &VersionedSettings) {
        *self.settings.lock().unwrap() = Some(settings.clone());
    }
    fn save_devices(&self, registry: &PairingRegistry) {
        *self.devices.lock().unwrap() = Some(registry.clone());
    }
    fn save_config(&self, config: &AppConfig) {
        *self.config.lock().unwrap() = Some(config.clone());
    }
    fn save_deck(&self, deck: &Deck) {
        *self.deck.lock().unwrap() = Some(deck.clone());
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Chaîne aléatoire sûre, encodée en base64url sans remplissage.
pub fn random_token(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rng().fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

struct Shared {
    settings: SettingsStore,
    registry: PairingRegistry,
    deck: Deck,
}

/// Les ports vers le système : saisie, loupe, programmes, presse-papiers.
pub struct Sinks {
    pub input: Arc<dyn InputSink>,
    pub magnifier: Arc<dyn MagnifierSink>,
    pub launcher: Arc<dyn Launcher>,
    pub clipboard: Arc<dyn ClipboardSink>,
}

/// Pourquoi un envoi du presse-papiers du téléphone a été refusé.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClipboardRefusal {
    #[error("appareil inconnu, appairez-le de nouveau")]
    Unauthorized,
    #[error("le partage du presse-papiers est désactivé dans les réglages")]
    Disabled,
    #[error(transparent)]
    Invalid(#[from] ClipboardError),
    #[error("{0}")]
    System(String),
}

pub struct AppState {
    shared: Mutex<Shared>,
    persist: Arc<dyn Persist>,
    pub input: Arc<dyn InputSink>,
    pub magnifier: Arc<dyn MagnifierSink>,
    pub launcher: Arc<dyn Launcher>,
    pub clipboard: Arc<dyn ClipboardSink>,
    settings_tx: broadcast::Sender<VersionedSettings>,
    revoked_tx: broadcast::Sender<String>,
    deck_tx: broadcast::Sender<DeckView>,
    connected: std::sync::atomic::AtomicUsize,
    actions_running: Arc<std::sync::atomic::AtomicUsize>,
    pub server_name: String,
}

/// Une place réservée parmi les actions en cours ; rendue à la destruction.
pub struct ActionSlot(Arc<std::sync::atomic::AtomicUsize>);

impl Drop for ActionSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

fn short_id() -> String {
    random_token(6)
}

impl AppState {
    pub fn new(
        settings: VersionedSettings,
        registry: PairingRegistry,
        deck: Deck,
        persist: Arc<dyn Persist>,
        sinks: Sinks,
        server_name: String,
    ) -> Arc<Self> {
        let (settings_tx, _) = broadcast::channel(16);
        let (revoked_tx, _) = broadcast::channel(16);
        let (deck_tx, _) = broadcast::channel(4);
        sinks.magnifier.configure(&settings.settings.magnifier);
        Arc::new(Self {
            shared: Mutex::new(Shared {
                settings: SettingsStore::new(settings.rev, settings.settings),
                registry,
                deck: deck.sanitized(&mut short_id),
            }),
            persist,
            input: sinks.input,
            magnifier: sinks.magnifier,
            launcher: sinks.launcher,
            clipboard: sinks.clipboard,
            settings_tx,
            revoked_tx,
            deck_tx,
            connected: std::sync::atomic::AtomicUsize::new(0),
            actions_running: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            server_name,
        })
    }

    // ----- Actions programmables -----

    pub fn deck(&self) -> Deck {
        self.lock().deck.clone()
    }

    pub fn deck_view(&self) -> DeckView {
        self.lock().deck.view()
    }

    pub fn subscribe_deck(&self) -> broadcast::Receiver<DeckView> {
        self.deck_tx.subscribe()
    }

    /// Remplace le panneau (depuis la fenêtre du PC uniquement), l'enregistre et le diffuse.
    pub fn set_deck(&self, deck: Deck) -> Deck {
        let deck = deck.sanitized(&mut short_id);
        let changed = {
            let mut shared = self.lock();
            let changed = shared.deck != deck;
            shared.deck = deck.clone();
            changed
        };
        if changed {
            self.persist.save_deck(&deck);
            let _ = self.deck_tx.send(deck.view());
        }
        deck
    }

    /// Valide une demande d'action : le bouton doit exister, les valeurs respecter ses champs.
    pub fn plan_action(
        &self,
        id: &str,
        values: &std::collections::BTreeMap<String, InputValue>,
    ) -> Result<ActionJob, ActionError> {
        let shared = self.lock();
        let button = shared.deck.button(id).ok_or(ActionError::UnknownAction)?;
        Ok(ActionJob {
            id: id.to_string(),
            plan: button.plan(values)?,
        })
    }

    /// Réserve une place pour une action, ou `None` s'il y en a déjà trop en cours.
    pub fn try_begin_action(&self) -> Option<ActionSlot> {
        use std::sync::atomic::Ordering;
        let mut n = self.actions_running.load(Ordering::Acquire);
        loop {
            if n >= MAX_CONCURRENT_ACTIONS {
                return None;
            }
            match self.actions_running.compare_exchange_weak(
                n,
                n + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(ActionSlot(self.actions_running.clone())),
                Err(current) => n = current,
            }
        }
    }

    /// Exécute une action (bloquant : les pauses dorment vraiment).
    pub fn run_job(&self, job: &ActionJob) -> Result<(), String> {
        let result = crate::actions::execute(
            &job.plan,
            self.input.as_ref(),
            self.launcher.as_ref(),
            &std::thread::sleep,
        );
        match &result {
            Ok(()) => tracing::info!(action = %job.id, "action exécutée"),
            Err(e) => tracing::warn!(action = %job.id, error = %e, "action en échec"),
        }
        result
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().expect("state lock poisoned")
    }

    pub fn settings(&self) -> VersionedSettings {
        self.lock().settings.current().clone()
    }

    /// Nombre de téléphones actuellement connectés et authentifiés.
    pub fn connected_count(&self) -> usize {
        self.connected.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub(crate) fn connection_opened(&self) {
        self.connected
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub(crate) fn connection_closed(&self) {
        self.connected
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Reçoit l'identifiant de chaque appareil révoqué, pour couper sa session en cours.
    pub fn subscribe_revocations(&self) -> broadcast::Receiver<String> {
        self.revoked_tx.subscribe()
    }

    pub fn subscribe_settings(&self) -> broadcast::Receiver<VersionedSettings> {
        self.settings_tx.subscribe()
    }

    /// Met à jour les réglages, les enregistre et les diffuse à tous les clients.
    pub fn update_settings(
        &self,
        base_rev: u64,
        settings: Settings,
    ) -> Result<VersionedSettings, Conflict> {
        let (updated, changed) = {
            let mut shared = self.lock();
            let before = shared.settings.current().rev;
            let updated = shared.settings.update(base_rev, settings)?.clone();
            let changed = updated.rev != before;
            (updated, changed)
        };
        if changed {
            self.persist.save_settings(&updated);
            self.magnifier.configure(&updated.settings.magnifier);
            let _ = self.settings_tx.send(updated.clone());
        }
        Ok(updated)
    }

    /// Émet un nouveau code d'appairage.
    pub fn new_pairing_code(&self) -> String {
        let code = random_token(16);
        self.lock().registry.issue_code(code.clone(), now_secs());
        code
    }

    /// Échange un code contre un jeton d'appareil. Renvoie `(id, jeton)`.
    pub fn pair(&self, code: &str, device_name: &str) -> Result<(String, String), PairingError> {
        let token = random_token(32);
        let id = random_token(9);
        let registry = {
            let mut shared = self.lock();
            shared
                .registry
                .redeem(code, device_name, id.clone(), &token, now_secs())?;
            shared.registry.clone()
        };
        self.persist.save_devices(&registry);
        Ok((id, token))
    }

    /// Place dans le presse-papiers du PC ce qu'un téléphone appairé a copié.
    /// Bloquant (décodage d'image, accès au presse-papiers) : à appeler hors de la boucle async.
    pub fn clipboard_from_phone(
        &self,
        token: &str,
        content_type: &str,
        body: &[u8],
    ) -> Result<(), ClipboardRefusal> {
        let device = self
            .authenticate(token)
            .ok_or(ClipboardRefusal::Unauthorized)?;
        if !self.settings().settings.clipboard.to_pc {
            return Err(ClipboardRefusal::Disabled);
        }
        let content = crate::clipboard::decode(content_type, body)?;
        self.clipboard
            .set(&content)
            .map_err(ClipboardRefusal::System)?;
        tracing::info!(device = %device.name, "presse-papiers reçu du téléphone");
        Ok(())
    }

    pub fn authenticate(&self, token: &str) -> Option<Device> {
        self.lock()
            .registry
            .authenticate(token, now_secs())
            .cloned()
    }

    pub fn devices(&self) -> Vec<Device> {
        self.lock().registry.devices().to_vec()
    }

    pub fn revoke(&self, device_id: &str) -> bool {
        let (removed, registry) = {
            let mut shared = self.lock();
            let removed = shared.registry.revoke(device_id);
            (removed, shared.registry.clone())
        };
        if removed {
            self.persist.save_devices(&registry);
            let _ = self.revoked_tx.send(device_id.to_string());
        }
        removed
    }
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use crate::actions::testing::FakeLauncher;
    use crate::clipboard::ClipboardContent;
    use crate::input::{apply_action, MagnifierStatus};
    use pwamote_core::magnifier::MagnifierController;
    use pwamote_core::pointer::WheelDelta;
    use pwamote_core::protocol::{Key, MagnifierAction, Modifier, MouseButton};
    use pwamote_core::settings::MagnifierSettings;

    #[derive(Debug, Clone, PartialEq)]
    pub enum Event {
        Move(i32, i32),
        Button(MouseButton, bool),
        Wheel(WheelDelta),
        Key(Key, bool),
        Modifier(Modifier, bool),
        Text(String),
        Recenter,
    }

    #[derive(Default)]
    pub struct FakeInput {
        pub events: Mutex<Vec<Event>>,
    }

    impl FakeInput {
        pub fn take(&self) -> Vec<Event> {
            std::mem::take(&mut *self.events.lock().unwrap())
        }
        fn push(&self, e: Event) {
            self.events.lock().unwrap().push(e);
        }
    }

    impl InputSink for FakeInput {
        fn move_by(&self, dx: i32, dy: i32) {
            self.push(Event::Move(dx, dy));
        }
        fn button(&self, b: MouseButton, down: bool) {
            self.push(Event::Button(b, down));
        }
        fn wheel(&self, d: WheelDelta) {
            self.push(Event::Wheel(d));
        }
        fn key(&self, k: Key, down: bool) {
            self.push(Event::Key(k, down));
        }
        fn modifier(&self, m: Modifier, down: bool) {
            self.push(Event::Modifier(m, down));
        }
        fn text(&self, t: &str) {
            self.push(Event::Text(t.into()));
        }
        fn recenter(&self) {
            self.push(Event::Recenter);
        }
    }

    pub struct FakeMagnifier(pub Mutex<MagnifierController>);

    impl MagnifierSink for FakeMagnifier {
        fn command(&self, action: MagnifierAction) -> MagnifierStatus {
            let mut m = self.0.lock().unwrap();
            apply_action(&mut m, action);
            MagnifierStatus {
                active: m.is_active(),
                zoom: m.target_zoom(),
            }
        }
        fn configure(&self, s: &MagnifierSettings) {
            self.0.lock().unwrap().set_zoom(s.zoom);
        }
    }

    #[derive(Default)]
    pub struct FakeClipboard {
        pub contents: Mutex<Vec<ClipboardContent>>,
        /// Erreur à renvoyer au prochain envoi, pour simuler un presse-papiers verrouillé.
        pub fail: Mutex<Option<String>>,
    }

    impl FakeClipboard {
        pub fn take(&self) -> Vec<ClipboardContent> {
            std::mem::take(&mut *self.contents.lock().unwrap())
        }
    }

    impl ClipboardSink for FakeClipboard {
        fn set(&self, content: &ClipboardContent) -> Result<(), String> {
            if let Some(e) = self.fail.lock().unwrap().take() {
                return Err(e);
            }
            self.contents.lock().unwrap().push(content.clone());
            Ok(())
        }
    }

    pub struct Fixture {
        pub state: Arc<AppState>,
        pub input: Arc<FakeInput>,
        pub persist: Arc<MemoryPersist>,
        pub launcher: Arc<FakeLauncher>,
        pub clipboard: Arc<FakeClipboard>,
    }

    pub fn fixture() -> Fixture {
        let input = Arc::new(FakeInput::default());
        let persist = Arc::new(MemoryPersist::default());
        let launcher = Arc::new(FakeLauncher::default());
        let clipboard = Arc::new(FakeClipboard::default());
        let magnifier = Arc::new(FakeMagnifier(Mutex::new(MagnifierController::new(
            &MagnifierSettings::default(),
        ))));
        let state = AppState::new(
            VersionedSettings {
                rev: 0,
                settings: Settings::default(),
            },
            PairingRegistry::default(),
            Deck::example(),
            persist.clone(),
            Sinks {
                input: input.clone(),
                magnifier,
                launcher: launcher.clone(),
                clipboard: clipboard.clone(),
            },
            "Test PC".into(),
        );
        Fixture {
            state,
            input,
            persist,
            launcher,
            clipboard,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::fixture;
    use super::*;
    use crate::clipboard::ClipboardContent;

    fn paired_token(f: &super::testing::Fixture) -> String {
        let code = f.state.new_pairing_code();
        f.state.pair(&code, "iPhone").unwrap().1
    }

    fn enable_clipboard(f: &super::testing::Fixture) {
        let mut s = f.state.settings().settings;
        s.clipboard.to_pc = true;
        let rev = f.state.settings().rev;
        f.state.update_settings(rev, s).unwrap();
    }

    #[test]
    fn clipboard_from_phone_is_refused_while_the_option_is_off() {
        let f = fixture();
        let token = paired_token(&f);
        assert_eq!(
            f.state.clipboard_from_phone(&token, "text/plain", b"hello"),
            Err(ClipboardRefusal::Disabled)
        );
        assert!(f.clipboard.take().is_empty());
    }

    #[test]
    fn clipboard_from_phone_needs_a_known_device() {
        let f = fixture();
        enable_clipboard(&f);
        assert_eq!(
            f.state.clipboard_from_phone("faux", "text/plain", b"hello"),
            Err(ClipboardRefusal::Unauthorized)
        );
        assert!(f.clipboard.take().is_empty());
    }

    #[test]
    fn clipboard_from_phone_reaches_the_system_clipboard() {
        let f = fixture();
        enable_clipboard(&f);
        let token = paired_token(&f);
        f.state
            .clipboard_from_phone(&token, "text/plain;charset=utf-8", "salut".as_bytes())
            .unwrap();
        assert_eq!(
            f.clipboard.take(),
            vec![ClipboardContent::Text("salut".into())]
        );
    }

    #[test]
    fn clipboard_from_phone_reports_bad_content_and_system_failures() {
        let f = fixture();
        enable_clipboard(&f);
        let token = paired_token(&f);
        assert!(matches!(
            f.state.clipboard_from_phone(&token, "image/png", b"nope"),
            Err(ClipboardRefusal::Invalid(_))
        ));
        *f.clipboard.fail.lock().unwrap() = Some("presse-papiers occupé".into());
        assert_eq!(
            f.state.clipboard_from_phone(&token, "text/plain", b"x"),
            Err(ClipboardRefusal::System("presse-papiers occupé".into()))
        );
    }

    #[test]
    fn pairing_persists_the_device_and_authenticates_it() {
        let f = fixture();
        let code = f.state.new_pairing_code();
        let (id, token) = f.state.pair(&code, "iPhone").unwrap();
        assert_eq!(f.state.authenticate(&token).unwrap().id, id);
        let saved = f.persist.devices.lock().unwrap().clone().unwrap();
        assert_eq!(saved.devices().len(), 1);
        assert!(!serde_json::to_string(&saved).unwrap().contains(&token));
    }

    #[test]
    fn settings_update_is_persisted_and_broadcast() {
        let f = fixture();
        let mut rx = f.state.subscribe_settings();
        let mut s = Settings::default();
        s.pointer.invert_y = true;
        let v = f.state.update_settings(0, s).unwrap();
        assert_eq!(v.rev, 1);
        assert_eq!(rx.try_recv().unwrap(), v);
        assert_eq!(f.persist.settings.lock().unwrap().clone().unwrap(), v);
    }

    #[test]
    fn identical_settings_are_not_rebroadcast() {
        let f = fixture();
        let mut rx = f.state.subscribe_settings();
        f.state.update_settings(0, Settings::default()).unwrap();
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn revoking_saves_the_registry() {
        let f = fixture();
        let code = f.state.new_pairing_code();
        let (id, token) = f.state.pair(&code, "A").unwrap();
        assert!(f.state.revoke(&id));
        assert!(f.state.authenticate(&token).is_none());
        assert_eq!(
            f.persist
                .devices
                .lock()
                .unwrap()
                .clone()
                .unwrap()
                .devices()
                .len(),
            0
        );
    }

    #[test]
    fn json_files_round_trip_and_tolerate_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let files = JsonFiles::new(dir.path()).unwrap();
        assert_eq!(files.load_settings().rev, 0);
        let mut s = Settings::default();
        s.magnifier.zoom = 3.0;
        files.save_settings(&VersionedSettings {
            rev: 7,
            settings: s.clone(),
        });
        let back = files.load_settings();
        assert_eq!((back.rev, back.settings), (7, s));

        std::fs::write(dir.path().join(SETTINGS_FILE), "{ not json").unwrap();
        assert_eq!(files.load_settings().rev, 0);
    }

    #[test]
    fn deck_changes_are_sanitized_persisted_and_broadcast() {
        let f = fixture();
        let mut rx = f.state.subscribe_deck();
        let mut deck = f.state.deck();
        deck.pages[0].name = "  Films  ".into();
        deck.pages[0].columns = 42;
        let saved = f.state.set_deck(deck);
        assert_eq!(saved.pages[0].name, "Films");
        assert_eq!(saved.pages[0].columns, pwamote_core::actions::MAX_COLUMNS);
        assert_eq!(f.persist.deck.lock().unwrap().clone().unwrap(), saved);
        assert_eq!(rx.try_recv().unwrap(), saved.view());
        // Même panneau : ni écriture ni diffusion.
        f.state.set_deck(saved);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn plan_action_checks_the_button_exists() {
        let f = fixture();
        let none = std::collections::BTreeMap::new();
        assert!(f.state.plan_action("slower", &none).is_ok());
        assert_eq!(
            f.state.plan_action("inconnu", &none),
            Err(ActionError::UnknownAction)
        );
    }

    #[test]
    fn concurrent_actions_are_capped() {
        let f = fixture();
        let slots: Vec<_> = (0..MAX_CONCURRENT_ACTIONS)
            .map(|_| f.state.try_begin_action().unwrap())
            .collect();
        assert!(f.state.try_begin_action().is_none());
        drop(slots);
        assert!(f.state.try_begin_action().is_some());
    }

    #[test]
    fn deck_file_starts_with_the_example_then_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let files = JsonFiles::new(dir.path()).unwrap();
        assert_eq!(files.load_deck(), Deck::example());
        files.save_deck(&Deck::default());
        assert_eq!(files.load_deck(), Deck::default());
    }

    #[test]
    fn random_tokens_are_unique_and_url_safe() {
        let a = random_token(32);
        let b = random_token(32);
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert!(a
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }
}
