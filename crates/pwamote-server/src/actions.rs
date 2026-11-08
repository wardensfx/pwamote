//! Exécution des actions programmables : touches, texte, programmes, ouvertures, pauses.
//!
//! Le plan (`PlannedStep`) est déjà validé par `pwamote-core` ; ici on ne fait que l'appliquer,
//! hors de la boucle WebSocket (une pause ne doit pas bloquer le pointeur).

use std::time::Duration;

use pwamote_core::actions::PlannedStep;

use crate::input::InputSink;

/// Nombre d'actions qui peuvent tourner en même temps, tous téléphones confondus.
pub const MAX_CONCURRENT_ACTIONS: usize = 4;

/// Lancement de programmes et ouverture de documents (remplaçable dans les tests).
pub trait Launcher: Send + Sync {
    /// Lance `program` avec ses arguments, sans interpréteur de commandes, sans attendre la fin.
    fn run(
        &self,
        program: &str,
        args: &[String],
        cwd: Option<&str>,
        hidden: bool,
    ) -> Result<(), String>;
    /// Ouvre une adresse, un fichier ou un dossier avec l'application par défaut.
    fn open(&self, target: &str) -> Result<(), String>;
}

/// Une action validée, prête à partir.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionJob {
    pub id: String,
    pub plan: Vec<PlannedStep>,
}

/// Applique un plan étape par étape. S'arrête à la première erreur.
pub fn execute(
    plan: &[PlannedStep],
    input: &dyn InputSink,
    launcher: &dyn Launcher,
    sleep: &dyn Fn(Duration),
) -> Result<(), String> {
    for step in plan {
        match step {
            PlannedStep::Keys { key, modifiers } => {
                for m in modifiers {
                    input.modifier(*m, true);
                }
                input.key(*key, true);
                input.key(*key, false);
                for m in modifiers.iter().rev() {
                    input.modifier(*m, false);
                }
            }
            PlannedStep::Text(text) => input.text(text),
            PlannedStep::Run {
                program,
                args,
                cwd,
                hidden,
            } => launcher.run(program, args, cwd.as_deref(), *hidden)?,
            PlannedStep::Open(target) => launcher.open(target)?,
            PlannedStep::Delay(ms) => sleep(Duration::from_millis(u64::from(*ms))),
        }
    }
    Ok(())
}

/// Vrais processus. Sous Windows, l'ouverture passe par `ShellExecuteW` ; ailleurs elle est
/// seulement journalisée (le serveur hors Windows sert au développement).
pub struct SystemLauncher;

impl Launcher for SystemLauncher {
    fn run(
        &self,
        program: &str,
        args: &[String],
        cwd: Option<&str>,
        hidden: bool,
    ) -> Result<(), String> {
        let mut cmd = std::process::Command::new(program);
        cmd.args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            if hidden {
                cmd.creation_flags(CREATE_NO_WINDOW);
            }
        }
        #[cfg(not(windows))]
        let _ = hidden;
        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                format!("programme introuvable : {program}")
            } else {
                format!("impossible de lancer {program} : {e}")
            }
        })?;
        tracing::info!(program, ?args, "programme lancé");
        // On n'attend pas la fin, mais on récupère le code de sortie pour ne rien laisser traîner.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }

    fn open(&self, target: &str) -> Result<(), String> {
        #[cfg(windows)]
        {
            crate::platform::windows::shell_open(target)
        }
        #[cfg(not(windows))]
        {
            tracing::info!(target, "ouverture (journalisée seulement hors Windows)");
            Ok(())
        }
    }
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use std::sync::Mutex;

    /// Lanceur qui note ce qu'on lui demande.
    #[derive(Default)]
    pub struct FakeLauncher {
        pub calls: Mutex<Vec<String>>,
        pub fail: Mutex<Option<String>>,
    }

    impl FakeLauncher {
        pub fn take(&self) -> Vec<String> {
            std::mem::take(&mut *self.calls.lock().unwrap())
        }
        fn result(&self) -> Result<(), String> {
            match self.fail.lock().unwrap().clone() {
                Some(e) => Err(e),
                None => Ok(()),
            }
        }
    }

    impl Launcher for FakeLauncher {
        fn run(
            &self,
            program: &str,
            args: &[String],
            cwd: Option<&str>,
            hidden: bool,
        ) -> Result<(), String> {
            self.calls.lock().unwrap().push(format!(
                "run {program} {args:?} cwd={cwd:?} hidden={hidden}"
            ));
            self.result()
        }
        fn open(&self, target: &str) -> Result<(), String> {
            self.calls.lock().unwrap().push(format!("open {target}"));
            self.result()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::FakeLauncher;
    use super::*;
    use crate::state::testing::{Event, FakeInput};
    use pwamote_core::protocol::{Key, Modifier};
    use std::sync::Mutex;

    #[test]
    fn steps_run_in_order_with_modifiers_wrapped_around_keys() {
        let input = FakeInput::default();
        let launcher = FakeLauncher::default();
        let slept = Mutex::new(Vec::new());
        let plan = vec![
            PlannedStep::Keys {
                key: Key::Char('s'),
                modifiers: vec![Modifier::Meta, Modifier::Shift],
            },
            PlannedStep::Delay(250),
            PlannedStep::Text("bonjour".into()),
            PlannedStep::Run {
                program: "pwsh".into(),
                args: vec!["-File".into(), "x.ps1".into()],
                cwd: Some("C:\\scripts".into()),
                hidden: true,
            },
            PlannedStep::Open("https://example.org".into()),
        ];
        execute(&plan, &input, &launcher, &|d| slept.lock().unwrap().push(d)).unwrap();
        assert_eq!(
            input.take(),
            vec![
                Event::Modifier(Modifier::Meta, true),
                Event::Modifier(Modifier::Shift, true),
                Event::Key(Key::Char('s'), true),
                Event::Key(Key::Char('s'), false),
                Event::Modifier(Modifier::Shift, false),
                Event::Modifier(Modifier::Meta, false),
                Event::Text("bonjour".into()),
            ]
        );
        assert_eq!(*slept.lock().unwrap(), vec![Duration::from_millis(250)]);
        assert_eq!(
            launcher.take(),
            vec![
                r#"run pwsh ["-File", "x.ps1"] cwd=Some("C:\\scripts") hidden=true"#.to_string(),
                "open https://example.org".into(),
            ]
        );
    }

    #[test]
    fn execution_stops_at_the_first_failure() {
        let input = FakeInput::default();
        let launcher = FakeLauncher::default();
        *launcher.fail.lock().unwrap() = Some("programme introuvable : x".into());
        let plan = vec![
            PlannedStep::Open("a".into()),
            PlannedStep::Text("jamais".into()),
        ];
        let err = execute(&plan, &input, &launcher, &|_| {}).unwrap_err();
        assert_eq!(err, "programme introuvable : x");
        assert!(input.take().is_empty());
    }

    #[test]
    fn system_launcher_reports_a_missing_program() {
        let err = SystemLauncher
            .run("pwamote-programme-qui-n-existe-pas", &[], None, true)
            .unwrap_err();
        assert!(err.contains("introuvable"), "{err}");
    }
}
